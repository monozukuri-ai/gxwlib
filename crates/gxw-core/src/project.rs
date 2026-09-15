use crate::{
    GxwError, ProjectIndex, ReadLimits, SourceInfo, SourceSpan, StreamLocation, source::SourceStore,
};
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FramingStatus {
    Unsupported,
    Partial,
    Complete,
}

impl FramingStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::Partial => "partial",
            Self::Complete => "complete",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ParseDiagnostic {
    pub code: String,
    pub severity: String,
    pub message: String,
    pub source: Option<SourceSpan>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpaqueRegion {
    pub source: SourceSpan,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RawToken {
    pub ordinal: usize,
    pub source: SourceSpan,
}

#[derive(Debug, Clone, Serialize)]
pub struct RawProgram {
    pub logical_index: usize,
    pub logical_name: Option<String>,
    pub metadata_source: SourceSpan,
    pub source: Option<SourceSpan>,
    pub profile: Option<String>,
    pub framing_status: FramingStatus,
    pub token_region: Option<SourceSpan>,
    pub trailer: Option<SourceSpan>,
    pub tokens: Vec<RawToken>,
    pub opaque_regions: Vec<OpaqueRegion>,
    pub diagnostics: Vec<ParseDiagnostic>,
}

#[derive(Debug)]
pub struct ParsedProject {
    pub index: Arc<ProjectIndex>,
    pub programs: Vec<RawProgram>,
    pub diagnostics: Vec<ParseDiagnostic>,
    store: Arc<SourceStore>,
}

impl ParsedProject {
    pub fn sources(&self) -> &[SourceInfo] {
        &self.store.infos
    }
    pub fn source_bytes(&self, span: &SourceSpan) -> Option<&[u8]> {
        self.store.bytes(span)
    }
    pub fn framing_complete(&self) -> bool {
        !self.programs.is_empty()
            && self
                .programs
                .iter()
                .all(|p| p.framing_status == FramingStatus::Complete)
    }
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let mut programs = Vec::new();
        for program in &self.programs {
            let mut value = serde_json::to_value(program)?;
            for (json, token) in value["tokens"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .zip(&program.tokens)
            {
                json["data_hex"] = spaced_hex(
                    self.source_bytes(&token.source)
                        .expect("validated token range"),
                )
                .into();
            }
            programs.push(value);
        }
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 1, "index": &*self.index, "sources": self.sources(), "programs": programs,
            "diagnostics": self.diagnostics, "framing_complete": self.framing_complete(),
            "semantic_status": "not_decoded", "cpu_model": null, "execution_order": null,
        }))
    }
}

pub(crate) fn spaced_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn parse(
    index: ProjectIndex,
    streams: crate::RetainedStreams,
    limits: &ReadLimits,
) -> Result<ParsedProject, GxwError> {
    limits.check("retained sources", streams.len() as u64, u32::MAX as u64)?;
    let store = Arc::new(SourceStore::new(streams));
    let metadata_id = store
        .find(&StreamLocation {
            container: vec![],
            path: vec!["projectdatalist.xml".into()],
        })
        .expect("required metadata retained");
    let mut diagnostics = vec![ParseDiagnostic { code: "GXW_RAW_ONLY".into(), severity: "info".into(), message: "Raw record framing only; instruction meanings, CPU settings and execution order are not decoded.".into(), source: None }];
    diagnostics.extend(
        index
            .diagnostics
            .iter()
            .filter(|d| d.code != "GXW_INSPECT_ONLY")
            .map(|d| ParseDiagnostic {
                code: d.code.clone(),
                severity: d.severity.clone(),
                message: d.message.clone(),
                source: d
                    .location
                    .as_ref()
                    .and_then(|l| store.find(l))
                    .map(|id| store.whole(id)),
            }),
    );
    let mut programs = Vec::new();
    let mut total = 0;
    for (logical_index, object) in index.logical_objects.iter().enumerate() {
        if object.historical || object.scrap == Some(true) {
            continue;
        }
        let fields = &object.metadata.fields;
        let kind_matches = fields.get("ucFolderType").is_some_and(|s| s == "7")
            && fields.get("ucFileType").is_some_and(|s| s == "2");
        let named_pou = object
            .logical_name
            .as_ref()
            .is_some_and(|s| s.to_ascii_lowercase().ends_with(".pou"));
        if !kind_matches && !named_pou {
            continue;
        }
        let range = &object.metadata.source_range;
        let metadata_source = store
            .span(metadata_id, range.start, range.end)
            .expect("validated XML range");
        let source = object
            .location
            .as_ref()
            .and_then(|l| store.find(l))
            .map(|id| store.whole(id));
        let mut program = RawProgram {
            logical_index,
            logical_name: object.logical_name.clone(),
            metadata_source,
            source,
            profile: None,
            framing_status: FramingStatus::Unsupported,
            token_region: None,
            trailer: None,
            tokens: Vec::new(),
            opaque_regions: Vec::new(),
            diagnostics: Vec::new(),
        };
        if let Some(source) = program.source.clone() {
            if kind_matches && index.containers.iter().all(|c| c.cfb_version == 3) {
                crate::formats::simple::tokenize(
                    &mut program,
                    store.bytes(&source).expect("source exists"),
                    limits,
                    &mut total,
                )?;
            } else {
                program.diagnostics.push(ParseDiagnostic {
                    code: "GXW_POU_METADATA_UNSUPPORTED".into(),
                    severity: "warning".into(),
                    message:
                        "POU candidate metadata or CFB version is outside the observed profile"
                            .into(),
                    source: Some(program.metadata_source.clone()),
                });
                program.opaque_regions.push(OpaqueRegion {
                    source: source.clone(),
                    reason: "unsupported_metadata".into(),
                });
            }
        } else {
            program.diagnostics.push(ParseDiagnostic {
                code: "GXW_POU_UNRESOLVED".into(),
                severity: "warning".into(),
                message: "POU candidate has no unambiguous current stream".into(),
                source: Some(program.metadata_source.clone()),
            });
        }
        diagnostics.extend(program.diagnostics.iter().cloned());
        programs.push(program);
    }
    Ok(ParsedProject {
        index: Arc::new(index),
        programs,
        diagnostics,
        store,
    })
}
