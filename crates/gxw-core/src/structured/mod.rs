//! Bounded, read-only structured POU geometry and declarations.
//!
//! Binary layout and geometric connectivity adapted from gxworks-agent
//! d758bc3106b711e4a2090e95bda119c3a2af06de (Apache-2.0).
//! See LICENSES/gxworks-agent.txt. No FB evaluation or CPU semantics are inferred.
mod connectivity;
mod declarations;
pub mod devices;
mod parser;
pub mod render;

use crate::{GxwError, OpaqueRegion, ParseDiagnostic, ParsedProject, SourceSpan};
use serde::Serialize;
use std::sync::Arc;

pub use declarations::{DeclarationTable, LabelDeclaration, decode_declarations};
pub use parser::decode_structured;

#[derive(Debug, Clone)]
pub struct StructuredLimits {
    pub max_blocks: u64,
    pub max_records: u64,
    pub max_ports: u64,
    pub max_string_units: u64,
    pub max_declarations: u64,
    pub max_connection_checks: u64,
}
impl Default for StructuredLimits {
    fn default() -> Self {
        Self {
            max_blocks: 4096,
            max_records: 100_000,
            max_ports: 200_000,
            max_string_units: 65_536,
            max_declarations: 65_536,
            max_connection_checks: 5_000_000,
        }
    }
}
pub(crate) fn check(resource: &str, actual: u64, limit: u64) -> Result<(), GxwError> {
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
pub(crate) fn unsupported(message: &str) -> GxwError {
    GxwError::Unsupported {
        context: "structured GXW".into(),
        message: message.into(),
    }
}
pub(crate) fn diagnostic(code: &str, message: &str, source: &SourceSpan) -> ParseDiagnostic {
    ParseDiagnostic {
        code: code.into(),
        severity: "warning".into(),
        message: message.into(),
        source: Some(source.clone()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Point {
    pub x: u32,
    pub y: u32,
}
#[derive(Debug, Clone, Serialize)]
pub struct StructuredPort {
    pub index: usize,
    pub kind_code: u32,
    pub local: Point,
    pub position: Point,
    pub source: SourceSpan,
}
#[derive(Debug, Clone, Serialize)]
pub struct StructuredNode {
    /// Record ordinal within its block (shared with wires/opaque records).
    pub ordinal: usize,
    pub kind_code: u32,
    pub kind: String,
    pub symbol: String,
    /// For FB nodes, symbol is the instance name and type_name the stored FB type.
    pub type_name: Option<String>,
    pub object_flag: Option<u32>,
    pub reserved: Option<u16>,
    pub bounds: [u32; 4],
    pub ports: Vec<StructuredPort>,
    pub source: SourceSpan,
}
#[derive(Debug, Clone, Serialize)]
pub struct StructuredWire {
    pub ordinal: usize,
    pub start: Point,
    pub end: Point,
    pub flags: [u32; 5],
    pub suffix: u32,
    pub source: SourceSpan,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct PortRef {
    pub node: usize,
    pub port: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct StructuredNet {
    pub index: usize,
    pub ports: Vec<PortRef>,
    /// Indices into this block's wires, not record ordinals.
    pub wires: Vec<usize>,
}
#[derive(Debug, Clone, Serialize)]
pub struct StructuredBlock {
    pub index: usize,
    pub source: SourceSpan,
    pub header: SourceSpan,
    pub canvas_height: u32,
    pub record_count: u32,
    pub nodes: Vec<StructuredNode>,
    pub wires: Vec<StructuredWire>,
    pub opaque_records: Vec<OpaqueRegion>,
    pub nets: Vec<StructuredNet>,
    pub connectivity_complete: bool,
}
#[derive(Debug, Serialize)]
pub struct StructuredProgram {
    pub schema_version: u32,
    pub source_sha256: String,
    pub logical_name: Option<String>,
    pub logical_index: usize,
    pub profile: String,
    pub source: SourceSpan,
    pub header: SourceSpan,
    pub trailer: SourceSpan,
    /// True only for known record layouts/flags. Does not imply executable semantics.
    pub structure_complete: bool,
    pub semantic_status: String,
    pub blocks: Vec<StructuredBlock>,
    pub diagnostics: Vec<ParseDiagnostic>,
    #[serde(skip)]
    pub(crate) project: Arc<ParsedProject>,
}
impl StructuredProgram {
    pub fn project(&self) -> &Arc<ParsedProject> {
        &self.project
    }
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// Shared IR entry point. Structured geometry is never lowered into scan instructions.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "program", rename_all = "snake_case")]
pub enum ProgramIr {
    Instructions(Box<crate::InstructionProgram>),
    Structured(Box<StructuredProgram>),
}
pub fn decode_ir(
    project: Arc<ParsedProject>,
    index: usize,
    device_profile: crate::DeviceProfile,
    limits: &StructuredLimits,
) -> Result<ProgramIr, GxwError> {
    let raw = project
        .programs
        .get(index)
        .ok_or_else(|| unsupported("program index out of range"))?;
    let data = raw.source.as_ref().and_then(|s| project.source_bytes(s));
    if data.and_then(|b| b.get(54)) == Some(&0xd0) {
        decode_structured(project, index, limits).map(|p| ProgramIr::Structured(Box::new(p)))
    } else {
        crate::decode_program(project, index, device_profile)
            .map(|p| ProgramIr::Instructions(Box::new(p)))
    }
}
