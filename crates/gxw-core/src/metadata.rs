use crate::{
    ContainerIndex, Diagnostic, GxwError, LogicalObject, MetadataRow, ReadLimits, StreamLocation,
};
use quick_xml::{NsReader, events::Event, name::ResolveResult};
use std::collections::BTreeMap;

fn text(data: &[u8], context: &str) -> Result<(String, &'static str, usize), GxwError> {
    let (body, encoding) = if data.starts_with(&[0xff, 0xfe]) {
        (&data[2..], "utf-16le")
    } else if data.starts_with(&[0xfe, 0xff]) {
        (&data[2..], "utf-16be")
    } else if data.starts_with(b"<\0") {
        (data, "utf-16le")
    } else if data.starts_with(b"\0<") {
        (data, "utf-16be")
    } else {
        (
            data.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(data),
            "utf-8",
        )
    };
    let decoded = if encoding == "utf-8" {
        String::from_utf8(body.to_vec()).map_err(|e| GxwError::format(context, e))?
    } else {
        if !body.len().is_multiple_of(2) {
            return Err(GxwError::format(context, "odd UTF-16 byte length"));
        }
        let words: Vec<u16> = body
            .chunks_exact(2)
            .map(|c| {
                if encoding == "utf-16le" {
                    u16::from_le_bytes([c[0], c[1]])
                } else {
                    u16::from_be_bytes([c[0], c[1]])
                }
            })
            .collect();
        String::from_utf16(&words).map_err(|e| GxwError::format(context, e))?
    };
    if decoded.chars().any(|c| !xml_char(c)) {
        return Err(GxwError::format(context, "invalid XML character"));
    }
    Ok((decoded, encoding, data.len() - body.len()))
}

/// Advances once through the decoded text, mapping event boundaries back to
/// original bytes, including BOM and surrogate pairs. No per-character table.
struct OffsetMapper<'a> {
    text: &'a str,
    encoding: &'static str,
    decoded: usize,
    original: usize,
}
impl OffsetMapper<'_> {
    fn at(&mut self, position: u64) -> usize {
        let position = (position as usize).min(self.text.len());
        debug_assert!(position >= self.decoded);
        let text = &self.text[self.decoded..position];
        self.original += if self.encoding == "utf-8" {
            text.len()
        } else {
            text.encode_utf16().count() * 2
        };
        self.decoded = position;
        self.original
    }
}

fn xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

pub(crate) fn rows(
    data: &[u8],
    row_name: &str,
    context: &str,
    limits: &ReadLimits,
) -> Result<Vec<MetadataRow>, GxwError> {
    limits.check("XML bytes", data.len() as u64, limits.max_xml_bytes)?;
    let (decoded, encoding, bom) = text(data, context)?;
    let mut mapper = OffsetMapper {
        text: &decoded,
        encoding,
        decoded: 0,
        original: bom,
    };
    let mut reader = NsReader::from_str(&decoded);
    reader.config_mut().expand_empty_elements = true;
    reader.config_mut().check_comments = true;
    let mut stack: Vec<String> = Vec::new();
    let mut history_stack: Vec<bool> = Vec::new();
    let mut namespace_stack: Vec<Option<String>> = Vec::new();
    let mut result = Vec::new();
    let mut current: Option<(usize, MetadataRow)> = None;
    let mut field: Option<(String, String)> = None;
    let mut roots = 0;
    let mut declaration = false;
    loop {
        let start = mapper.at(reader.buffer_position());
        let event = reader.read_event().map_err(|e| {
            GxwError::format(
                format!(
                    "{context} at decoded UTF-8 offset {}",
                    reader.error_position()
                ),
                e,
            )
        })?;
        match event {
            Event::Start(e) => {
                if stack.is_empty() {
                    roots += 1;
                }
                if roots > 1 {
                    return Err(GxwError::format(context, "multiple XML roots"));
                }
                limits.check("XML depth", stack.len() as u64 + 1, limits.max_xml_depth)?;
                let namespace = match reader.resolver().resolve_element(e.name()).0 {
                    ResolveResult::Unbound => None,
                    ResolveResult::Bound(ns) => Some(ns.as_ref().to_owned()),
                    ResolveResult::Unknown(prefix) => {
                        return Err(GxwError::format(
                            context,
                            format!("unbound XML prefix: {prefix}"),
                        ));
                    }
                };
                let historical = history_stack.last().copied().unwrap_or(false)
                    || (namespace.as_deref() == Some("urn:schemas-microsoft-com:xml-diffgram-v1")
                        && matches!(e.local_name().as_ref(), "before" | "errors"));
                let mut attributes = BTreeMap::new();
                for a in e.attributes() {
                    let a = a.map_err(|e| GxwError::format(context, e))?;
                    let value = a
                        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                        .map_err(|e| GxwError::format(context, e))?
                        .into_owned();
                    if value.chars().any(|c| !xml_char(c)) {
                        return Err(GxwError::format(context, "invalid XML attribute character"));
                    }
                    if a.key.as_ref() != "xmlns"
                        && !a.key.as_ref().starts_with("xmlns:")
                        && let ResolveResult::Unknown(prefix) =
                            reader.resolver().resolve_attribute(a.key).0
                    {
                        return Err(GxwError::format(
                            context,
                            format!("unbound attribute prefix: {prefix}"),
                        ));
                    }
                    attributes.insert(a.key.as_ref().to_owned(), value);
                }
                if e.local_name().as_ref() == row_name {
                    if current.is_some() {
                        return Err(GxwError::format(context, "nested metadata row"));
                    }
                    limits.check("XML rows", result.len() as u64 + 1, limits.max_xml_rows)?;
                    current = Some((
                        stack.len() + 1,
                        MetadataRow {
                            fields: BTreeMap::new(),
                            attributes,
                            ancestors: stack.clone(),
                            source_range: start..start,
                            historical,
                            schema_compatible: namespace.is_none()
                                && namespace_stack.last().is_some_and(|n| n.is_none())
                                && stack.last().is_some_and(|s| {
                                    matches!(
                                        s.as_str(),
                                        "DSPROJECTDATA"
                                            | "DSPROJECT"
                                            | "diffgr:before"
                                            | "diffgr:errors"
                                    )
                                }),
                        },
                    ));
                } else if let Some((depth, row)) = &mut current {
                    if stack.len() != *depth {
                        return Err(GxwError::Unsupported {
                            context: context.into(),
                            message: "nested metadata field".into(),
                        });
                    }
                    field = Some((e.name().as_ref().to_owned(), String::new()));
                    if namespace.is_some() {
                        row.schema_compatible = false;
                    }
                }
                stack.push(e.name().as_ref().to_owned());
                namespace_stack.push(namespace);
                history_stack.push(historical);
            }
            Event::End(_) => {
                if let Some((depth, row)) = &mut current {
                    if stack.len() == *depth + 1 {
                        if let Some((name, value)) = field.take()
                            && row.fields.insert(name, value).is_some()
                        {
                            return Err(GxwError::format(context, "duplicate metadata field"));
                        }
                    } else if stack.len() == *depth {
                        row.source_range.end = mapper.at(reader.buffer_position());
                        result.push(current.take().expect("current row exists").1);
                    }
                }
                stack
                    .pop()
                    .ok_or_else(|| GxwError::format(context, "unmatched XML end"))?;
                namespace_stack.pop();
                history_stack.pop();
            }
            Event::Text(e) => append_text(&e.xml10_content(), &mut field, &stack, context)?,
            Event::CData(e) => append_text(&e.xml10_content(), &mut field, &stack, context)?,
            Event::GeneralRef(e) => {
                let value = match e
                    .resolve_char_ref()
                    .map_err(|e| GxwError::format(context, e))?
                {
                    Some(c) if xml_char(c) => c.to_string(),
                    Some(_) => {
                        return Err(GxwError::format(context, "invalid XML character reference"));
                    }
                    None => match &*e {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => return Err(GxwError::format(context, "unknown XML entity")),
                    }
                    .into(),
                };
                append_text(&value, &mut field, &stack, context)?;
            }
            Event::Decl(e) => {
                if declaration || roots != 0 {
                    return Err(GxwError::format(context, "misplaced XML declaration"));
                }
                declaration = true;
                if e.version()
                    .map_err(|e| GxwError::format(context, e))?
                    .as_ref()
                    != "1.0"
                {
                    return Err(GxwError::Unsupported {
                        context: context.into(),
                        message: "only XML 1.0 is supported".into(),
                    });
                }
                if let Some(enc) = e.encoding() {
                    let enc = enc
                        .map_err(|e| GxwError::format(context, e))?
                        .to_ascii_lowercase();
                    if enc != encoding && !(enc == "utf-16" && encoding.starts_with("utf-16")) {
                        return Err(GxwError::Unsupported {
                            context: context.into(),
                            message: format!(
                                "XML encoding declaration {enc} conflicts with {encoding}"
                            ),
                        });
                    }
                }
            }
            Event::DocType(_) => {
                return Err(GxwError::Unsupported {
                    context: context.into(),
                    message: "XML DTDs are not supported".into(),
                });
            }
            Event::Eof => {
                if !stack.is_empty() || roots != 1 {
                    return Err(GxwError::format(context, "incomplete XML document"));
                }
                break;
            }
            _ => {}
        }
    }
    Ok(result)
}

fn append_text(
    value: &str,
    field: &mut Option<(String, String)>,
    stack: &[String],
    context: &str,
) -> Result<(), GxwError> {
    if let Some((_, text)) = field {
        text.push_str(value);
    } else if stack.is_empty() && !value.trim().is_empty() {
        return Err(GxwError::format(context, "text outside XML root"));
    }
    Ok(())
}

pub(crate) fn resolve(
    rows: Vec<MetadataRow>,
    outer: &ContainerIndex,
    inner: &ContainerIndex,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<LogicalObject> {
    let root_streams = |index: &ContainerIndex| {
        index
            .streams
            .iter()
            .filter(|s| s.path.len() == 1)
            .map(|s| (s.path[0].clone(), s.clone()))
            .collect::<BTreeMap<_, _>>()
    };
    let outer_names = root_streams(outer);
    let inner_ids = root_streams(inner);
    let mut objects: Vec<LogicalObject> = rows
        .into_iter()
        .map(|row| {
            let historical = row.historical;
            let scrap = row.fields.get("bScrapFlag").and_then(|s| match s.as_str() {
                "true" | "1" => Some(true),
                "false" | "0" => Some(false),
                _ => None,
            });
            LogicalObject {
                id: row.fields.get("iID").cloned(),
                logical_name: row.fields.get("szName").cloned(),
                scrap,
                historical,
                metadata: row,
                location: None,
            }
        })
        .collect();
    let mut ids = BTreeMap::<String, usize>::new();
    for object in &objects {
        if !object.historical
            && object.scrap != Some(true)
            && let Some(id) = &object.id
        {
            *ids.entry(id.clone()).or_default() += 1;
        }
    }
    for object in &mut objects {
        if object.historical || object.scrap == Some(true) {
            continue;
        }
        let location = StreamLocation {
            container: vec![],
            path: vec!["projectdatalist.xml".into()],
        };
        let mut warn = |code: &str, message: String| {
            diagnostics.push(Diagnostic {
                code: code.into(),
                severity: "warning".into(),
                message,
                location: Some(location.clone()),
            })
        };
        if !object.metadata.schema_compatible {
            warn(
                "GXW_METADATA_SCHEMA_UNSUPPORTED",
                "metadata namespace or parent is outside the observed schema".into(),
            );
            continue;
        }
        if object.scrap.is_none() {
            warn(
                "GXW_ACTIVITY_UNKNOWN",
                format!("activity flag is unknown for {:?}", object.logical_name),
            );
            continue;
        }
        let (Some(id), Some(name)) = (&object.id, &object.logical_name) else {
            warn(
                "GXW_LOGICAL_FIELDS_MISSING",
                "logical row is missing iID or szName".into(),
            );
            continue;
        };
        if ids.get(id).copied().unwrap_or(0) > 1 {
            warn(
                "GXW_DUPLICATE_LOGICAL_ID",
                format!("multiple current rows use ID {id}"),
            );
            continue;
        }
        let a = inner_ids.get(id);
        let b = outer_names.get(name);
        object.location = match (a, b) {
            (Some(_), Some(_)) => {
                warn(
                    "GXW_AMBIGUOUS_LOCATION",
                    format!("both containers match {name}"),
                );
                None
            }
            (Some(s), None) => Some(StreamLocation {
                container: inner.path.clone(),
                path: s.path.clone(),
            }),
            (None, Some(s)) => Some(StreamLocation {
                container: outer.path.clone(),
                path: s.path.clone(),
            }),
            (None, None) => {
                warn(
                    "GXW_UNRESOLVED_LOGICAL_OBJECT",
                    format!("no stream for ID {id}: {name}"),
                );
                None
            }
        };
    }
    objects
}
