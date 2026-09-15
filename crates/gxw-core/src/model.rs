use crate::GxwError;
use serde::Serialize;
use std::collections::BTreeMap;

/// Resource bounds, not an exact process RSS budget. CFB allocation tables are
/// additionally bounded by the input size. Only the outer CFB and _hdb are opened.
#[derive(Debug, Clone)]
pub struct ReadLimits {
    pub max_file_bytes: u64,
    pub max_stream_bytes: u64,
    pub max_total_stream_bytes: u64,
    pub max_entries: u64,
    pub max_xml_bytes: u64,
    pub max_xml_depth: u64,
    pub max_xml_rows: u64,
    pub max_tokens: u64,
}

impl Default for ReadLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 64 << 20,
            max_stream_bytes: 64 << 20,
            max_total_stream_bytes: 256 << 20,
            max_entries: 65_536,
            max_xml_bytes: 8 << 20,
            max_xml_depth: 64,
            max_xml_rows: 65_536,
            max_tokens: 1_000_000,
        }
    }
}

impl ReadLimits {
    pub(crate) fn check(&self, resource: &str, actual: u64, limit: u64) -> Result<(), GxwError> {
        if actual > limit {
            Err(GxwError::ResourceLimit {
                resource: resource.into(),
                actual,
                limit,
            })
        } else {
            Ok(())
        }
    }
}

/// Paths are components within a CFB, independent of the host OS separator.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StreamInfo {
    pub path: Vec<String>,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContainerIndex {
    /// Empty for outer CFB; ["_hdb"] for the nested CFB.
    pub path: Vec<String>,
    pub cfb_version: u8,
    pub streams: Vec<StreamInfo>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct StreamLocation {
    pub container: Vec<String>,
    pub path: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MetadataRow {
    /// Original qualified names and values, without guessing schema types.
    pub fields: BTreeMap<String, String>,
    pub attributes: BTreeMap<String, String>,
    pub ancestors: Vec<String>,
    /// Original XML stream bytes. Kept out of the P0 inventory JSON contract.
    #[serde(skip)]
    pub(crate) source_range: std::ops::Range<usize>,
    #[serde(skip)]
    pub(crate) historical: bool,
    #[serde(skip)]
    pub(crate) schema_compatible: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LogicalObject {
    pub id: Option<String>,
    pub logical_name: Option<String>,
    pub scrap: Option<bool>,
    pub historical: bool,
    pub metadata: MetadataRow,
    pub location: Option<StreamLocation>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: String,
    pub severity: String,
    pub message: String,
    pub location: Option<StreamLocation>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProjectIndex {
    pub schema_version: u32,
    pub size: u64,
    pub sha256: String,
    pub containers: Vec<ContainerIndex>,
    pub logical_objects: Vec<LogicalObject>,
    pub project_rows: Vec<MetadataRow>,
    pub diagnostics: Vec<Diagnostic>,
}

impl ProjectIndex {
    /// Explicit export; Python getters use Rust-backed views instead.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}
