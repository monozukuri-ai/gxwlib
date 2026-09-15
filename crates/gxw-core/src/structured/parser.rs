// Adapted from gxworks-agent structured_pou.py (Apache-2.0); modified for bounded
// read-only Rust parsing and strict validation. See LICENSES/gxworks-agent.txt.
use super::*;

// Observed common header; timestamp bytes 34..50 remain opaque.
const PREFIX: [u8; 34] = [
    1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 10, 0, 0, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0,
    0, 0, 0,
];
pub(super) fn common_header(data: &[u8]) -> Result<(), GxwError> {
    if data.len() < 54
        || data[..34] != PREFIX
        || !matches!(&data[50..54], [0, 0, 0, 0] | [1, 0, 0, 0])
    {
        return Err(unsupported("unrecognized common header"));
    }
    Ok(())
}
pub(super) struct Reader<'a> {
    pub data: &'a [u8],
    pub pos: usize,
    pub end: usize,
    pub source_id: u32,
    pub limits: &'a StructuredLimits,
}
impl<'a> Reader<'a> {
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], GxwError> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|e| *e <= self.end)
            .ok_or_else(|| {
                GxwError::format("structured record", "truncated or overflowing field")
            })?;
        let value = &self.data[self.pos..end];
        self.pos = end;
        Ok(value)
    }
    pub fn u32(&mut self) -> Result<u32, GxwError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn u16(&mut self) -> Result<u16, GxwError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub fn text(&mut self) -> Result<String, GxwError> {
        let count = self.u32()? as u64;
        check(
            "structured string units",
            count,
            self.limits.max_string_units,
        )?;
        if count == 0 {
            return Err(GxwError::format(
                "structured string",
                "missing NUL terminator",
            ));
        }
        let size =
            usize::try_from(count * 2).map_err(|e| GxwError::format("structured string", e))?;
        let units: Vec<u16> = self
            .take(size)?
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        if units.last() != Some(&0) || units[..units.len() - 1].contains(&0) {
            return Err(GxwError::format(
                "structured string",
                "invalid NUL termination",
            ));
        }
        String::from_utf16(&units[..units.len() - 1])
            .map_err(|e| GxwError::format("structured UTF-16", e))
    }
    pub fn span(&self, start: usize, end: usize) -> SourceSpan {
        SourceSpan {
            source_id: self.source_id,
            offset: start as u64,
            length: (end - start) as u64,
        }
    }
}

pub fn decode_structured(
    project: Arc<ParsedProject>,
    index: usize,
    limits: &StructuredLimits,
) -> Result<StructuredProgram, GxwError> {
    let raw = project
        .programs
        .get(index)
        .ok_or_else(|| unsupported("program index out of range"))?;
    let object = &project.index.logical_objects[raw.logical_index];
    if project.index.containers.iter().any(|c| c.cfb_version != 3)
        || object
            .metadata
            .fields
            .get("ucFolderType")
            .map(String::as_str)
            != Some("7")
        || object.metadata.fields.get("ucFileType").map(String::as_str) != Some("2")
    {
        return Err(unsupported("unrecognized POU metadata/CFB version"));
    }
    let source = raw
        .source
        .clone()
        .ok_or_else(|| unsupported("unresolved POU stream"))?;
    let data = project
        .source_bytes(&source)
        .ok_or_else(|| unsupported("unavailable POU stream"))?;
    common_header(data)?;
    if data.len() < 95 || data[54] != 0xd0 || data[63..67] != [1, 0, 0, 0] {
        return Err(unsupported("unrecognized structured POU envelope"));
    }
    let mut r = Reader {
        data,
        pos: 55,
        end: data.len(),
        source_id: source.source_id,
        limits,
    };
    let length = r.u32()? as usize;
    if r.u32()? as usize != length || length != data.len() - 83 {
        return Err(GxwError::format(
            "structured POU",
            "inconsistent duplicated length",
        ));
    }
    r.pos = 67;
    let count = r.u32()? as u64;
    check("structured blocks", count, limits.max_blocks)?;
    if count == 0 {
        return Err(GxwError::format("structured POU", "zero blocks"));
    }
    r.end = data.len() - 24;
    if data[r.end..].iter().any(|b| *b != 0) {
        return Err(unsupported("unrecognized structured trailer"));
    }
    let mut blocks = Vec::new();
    let mut diagnostics = Vec::new();
    let (mut records, mut ports, mut checks) = (0u64, 0u64, 0u64);
    for index in 0..count as usize {
        let start = r.pos;
        let size = r.u32()? as usize;
        let block_end = start
            .checked_add(size)
            .filter(|e| size >= 24 && *e <= r.end)
            .ok_or_else(|| GxwError::format("structured block", "invalid length"))?;
        r.end = block_end;
        if r.take(12)? != [1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0] {
            return Err(unsupported("unrecognized block header"));
        }
        let canvas_height = r.u32()?;
        let record_count = r.u32()?;
        records += record_count as u64;
        check("structured records", records, limits.max_records)?;
        if record_count as u64 * 8 > (block_end - r.pos) as u64 {
            return Err(GxwError::format(
                "structured block",
                "impossible record count",
            ));
        }
        let mut block = StructuredBlock {
            index,
            source: r.span(start, block_end),
            header: r.span(start, start + 24),
            canvas_height,
            record_count,
            nodes: vec![],
            wires: vec![],
            opaque_records: vec![],
            nets: vec![],
            connectivity_complete: true,
        };
        for ordinal in 0..record_count as usize {
            let start = r.pos;
            let size = r.u32()? as usize;
            let end = start
                .checked_add(size)
                .filter(|e| size >= 8 && *e <= block_end)
                .ok_or_else(|| GxwError::format("structured record", "invalid length"))?;
            r.end = end;
            let class = r.u32()?;
            let span = r.span(start, end);
            match class {
                1 => {
                    let kind_code = r.u32()?;
                    // Unknown kinds may have a different layout: retain the whole record.
                    let kind = match kind_code {
                        1 => "function",
                        2 => "function_block",
                        3 => "contact",
                        4 => "contact_nc",
                        5 => "coil",
                        13 => "input",
                        14 => "output",
                        _ => "unknown",
                    };
                    if kind == "unknown" {
                        block.opaque_records.push(OpaqueRegion {
                            source: span.clone(),
                            reason: format!("unknown node kind {kind_code}"),
                        });
                        diagnostics.push(diagnostic(
                            "GXW_STRUCTURED_UNKNOWN_NODE",
                            "Unknown node layout retained as opaque",
                            &span,
                        ));
                        r.pos = end;
                        r.end = block_end;
                        continue;
                    }
                    let symbol = r.text()?;
                    let (type_name, object_flag, reserved) = if kind_code == 2 {
                        (Some(r.text()?), None, None)
                    } else {
                        (None, Some(r.u32()?), Some(r.u16()?))
                    };
                    let bounds = [r.u32()?, r.u32()?, r.u32()?, r.u32()?];
                    if bounds[0] > bounds[2] || bounds[1] > bounds[3] {
                        return Err(GxwError::format("structured node", "inverted bounds"));
                    }
                    let n = r.u32()? as u64;
                    ports += n;
                    check("structured ports", ports, limits.max_ports)?;
                    if n * 16 != (end - r.pos) as u64 {
                        return Err(GxwError::format(
                            "structured node",
                            "invalid port count/record size",
                        ));
                    }
                    let mut node = StructuredNode {
                        ordinal,
                        kind_code,
                        kind: kind.into(),
                        symbol,
                        type_name,
                        object_flag,
                        reserved,
                        bounds,
                        ports: vec![],
                        source: span.clone(),
                    };
                    for index in 0..n as usize {
                        let start = r.pos;
                        if r.u32()? != 16 {
                            return Err(unsupported("unrecognized port size"));
                        }
                        let kind_code = r.u32()?;
                        let local = Point {
                            x: r.u32()?,
                            y: r.u32()?,
                        };
                        let position = Point {
                            x: bounds[0]
                                .checked_add(local.x)
                                .ok_or_else(|| GxwError::format("port", "coordinate overflow"))?,
                            y: bounds[1]
                                .checked_add(local.y)
                                .ok_or_else(|| GxwError::format("port", "coordinate overflow"))?,
                        };
                        node.ports.push(StructuredPort {
                            index,
                            kind_code,
                            local,
                            position,
                            source: r.span(start, r.pos),
                        });
                    }
                    if object_flag.is_some_and(|v| v != 1) || reserved.is_some_and(|v| v != 0) {
                        diagnostics.push(diagnostic(
                            "GXW_STRUCTURED_NODE_FLAGS",
                            "Unknown node flags; connectivity is incomplete",
                            &span,
                        ));
                        block.connectivity_complete = false;
                    }
                    if kind_code == 1 || kind_code == 2 {
                        diagnostics.push(diagnostic("GXW_STRUCTURED_OPAQUE_FUNCTION", "Function/FB identity decoded; implementation and execution semantics remain opaque", &span));
                    }
                    block.nodes.push(node);
                }
                2 => {
                    if size != 44 {
                        return Err(unsupported("unrecognized wire size"));
                    }
                    let flags = [
                        r.u32()?,
                        r.u32()?,
                        r.u16()? as u32,
                        r.u16()? as u32,
                        r.u32()?,
                    ];
                    let start = Point {
                        x: r.u32()?,
                        y: r.u32()?,
                    };
                    let end = Point {
                        x: r.u32()?,
                        y: r.u32()?,
                    };
                    let suffix = r.u32()?;
                    if flags != [0, 1, 0, 1, 0]
                        || suffix != 0
                        || (start.x != end.x && start.y != end.y)
                    {
                        block.connectivity_complete = false;
                        diagnostics.push(diagnostic(
                            "GXW_STRUCTURED_WIRE_UNSUPPORTED",
                            "Unrecognized wire geometry/flags; connectivity is incomplete",
                            &span,
                        ));
                    }
                    block.wires.push(StructuredWire {
                        ordinal,
                        start,
                        end,
                        flags,
                        suffix,
                        source: span,
                    });
                }
                _ => {
                    block.opaque_records.push(OpaqueRegion {
                        source: span.clone(),
                        reason: format!("unknown record class {class}"),
                    });
                    diagnostics.push(diagnostic(
                        "GXW_STRUCTURED_UNKNOWN_RECORD",
                        "Unknown record retained as opaque",
                        &span,
                    ));
                    r.pos = end;
                }
            }
            if r.pos != end {
                return Err(GxwError::format(
                    "structured record",
                    "unconsumed record bytes",
                ));
            }
            r.end = block_end;
        }
        if r.pos != block_end {
            return Err(GxwError::format(
                "structured block",
                "unconsumed block bytes",
            ));
        }
        block.connectivity_complete &= block.opaque_records.is_empty();
        if block.connectivity_complete {
            block.nets = super::connectivity::connect(&block, limits, &mut checks)?;
        }
        blocks.push(block);
        r.end = data.len() - 24;
    }
    if r.pos != r.end {
        return Err(GxwError::format("structured POU", "unconsumed body bytes"));
    }
    Ok(StructuredProgram {
        schema_version: 1,
        source_sha256: project.index.sha256.clone(),
        logical_name: raw.logical_name.clone(),
        logical_index: raw.logical_index,
        profile: "gxw-structured-records-v1".into(),
        source,
        header: r.span(0, 71),
        trailer: r.span(r.end, data.len()),
        structure_complete: blocks.iter().all(|b| b.connectivity_complete),
        semantic_status: "not_evaluated".into(),
        blocks,
        diagnostics,
        project,
    })
}
