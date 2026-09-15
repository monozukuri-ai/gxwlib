// Adapted from gxworks-agent declarations.py (Apache-2.0); read-only Rust port
// with source spans, strict profile checks and limits. See LICENSES/gxworks-agent.txt.
use super::{
    parser::{Reader, common_header},
    *,
};

#[derive(Debug, Clone, Serialize)]
pub struct LabelDeclaration {
    pub ordinal: usize,
    pub name: String,
    pub data_type: String,
    pub class_code: u32,
    pub device: String,
    pub iec_address: String,
    pub unknown_u32: u32,
    pub initial_value: String,
    pub unknown_text: String,
    pub record_id: u32,
    pub comment: String,
    pub array_marker: u32,
    pub type_code: u32,
    pub type_reference: String,
    pub source: SourceSpan,
}
#[derive(Debug, Serialize)]
pub struct DeclarationTable {
    pub schema_version: u32,
    pub source_sha256: String,
    pub logical_index: usize,
    pub logical_name: String,
    pub scope: String,
    pub owner_name: Option<String>,
    pub source: SourceSpan,
    pub header: SourceSpan,
    pub trailer: SourceSpan,
    pub rows: Vec<LabelDeclaration>,
    /// Row layout only. Initial values, array bounds and type references are not evaluated.
    pub structure_complete: bool,
    pub diagnostics: Vec<ParseDiagnostic>,
    #[serde(skip)]
    pub(crate) project: Arc<ParsedProject>,
}
impl DeclarationTable {
    pub fn project(&self) -> &Arc<ParsedProject> {
        &self.project
    }
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// Decode an explicitly selected current logical object; historical/scrap rows are rejected.
pub fn decode_declarations(
    project: Arc<ParsedProject>,
    logical_index: usize,
    limits: &StructuredLimits,
) -> Result<DeclarationTable, GxwError> {
    let object = project
        .index
        .logical_objects
        .get(logical_index)
        .ok_or_else(|| unsupported("logical index out of range"))?;
    if object.historical
        || object.scrap == Some(true)
        || project.index.containers.iter().any(|c| c.cfb_version != 3)
    {
        return Err(unsupported("declarations must be current CFB v3 objects"));
    }
    let name = object.logical_name.as_deref().unwrap_or("");
    let scope = if name.ends_with(".Labels.lh") {
        "local"
    } else if name.ends_with(".gh") {
        "global"
    } else {
        return Err(unsupported("unrecognized declaration name"));
    };
    let info = project
        .sources()
        .iter()
        .find(|s| Some(&s.location) == object.location.as_ref())
        .ok_or_else(|| unsupported("unresolved declaration stream"))?;
    let source = SourceSpan {
        source_id: info.id,
        offset: 0,
        length: info.size,
    };
    let data = project.source_bytes(&source).unwrap();
    common_header(data)?;
    let mut r = Reader {
        data,
        pos: 54,
        end: data.len(),
        source_id: info.id,
        limits,
    };
    let owner_name = if scope == "local" {
        let owner = r.text()?;
        r.take(20)?;
        Some(owner)
    } else {
        None
    };
    let count = r.u32()? as u64;
    check("label declarations", count, limits.max_declarations)?;
    if count * 68 > (r.end - r.pos) as u64 {
        return Err(GxwError::format("declarations", "impossible row count"));
    }
    let header = r.span(0, r.pos);
    let mut rows = Vec::new();
    let mut diagnostics = Vec::new();
    let mut names = std::collections::BTreeSet::new();
    let mut ids = std::collections::BTreeSet::new();
    for ordinal in 0..count as usize {
        let start = r.pos;
        let row = LabelDeclaration {
            ordinal,
            name: r.text()?,
            data_type: r.text()?,
            class_code: r.u32()?,
            device: r.text()?,
            iec_address: r.text()?,
            unknown_u32: r.u32()?,
            initial_value: r.text()?,
            unknown_text: r.text()?,
            record_id: r.u32()?,
            comment: r.text()?,
            array_marker: r.u32()?,
            type_code: r.u32()?,
            type_reference: r.text()?,
            source: r.span(start, r.pos),
        };
        if !names.insert(row.name.to_lowercase()) || !ids.insert(row.record_id) {
            diagnostics.push(diagnostic(
                "GXW_DECLARATION_AMBIGUOUS",
                "Duplicate declaration name or record ID; no symbol resolution performed",
                &row.source,
            ));
        }
        rows.push(row);
    }
    let trailer = r.span(r.pos, r.end);
    let valid_trailer = if scope == "global" {
        r.pos == r.end
    } else {
        r.end - r.pos == 24 && data[r.pos + 16..] == [255, 0, 0, 0, 0, 0, 0, 0]
    };
    if !valid_trailer {
        diagnostics.push(diagnostic(
            "GXW_DECLARATION_TRAILER_UNSUPPORTED",
            "Unknown trailer retained; table layout is partial",
            &trailer,
        ));
    }
    Ok(DeclarationTable {
        schema_version: 1,
        source_sha256: project.index.sha256.clone(),
        logical_index,
        logical_name: name.into(),
        scope: scope.into(),
        owner_name,
        source,
        header,
        trailer,
        rows,
        structure_complete: valid_trailer,
        diagnostics,
        project,
    })
}
