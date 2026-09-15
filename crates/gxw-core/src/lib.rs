//! Read-only GX Works2 inspection and bounded raw POU framing.

pub mod analysis;
mod container;
mod error;
mod formats;
pub mod ir;
pub mod ladder;
pub mod simulation;
pub mod structured;
pub use analysis::diff::{DiffHunk, DiffKind, ProgramDiff, diff_programs};
pub use analysis::{
    AccessMode, AnalysisOptions, AnalysisReport, DeviceAccess, DeviceRef, DeviceUsage, Finding,
    analyze,
};
pub use ladder::render::{RenderOptions, SvgDocument, SvgElement, render_svg};
pub use ladder::{
    Condition, ConditionKind, LadderControl, LadderGraph, LadderOutput, build as build_ladder,
};
pub use simulation::{
    CompiledInstruction, CompiledProgram, CounterState, InstructionEvent, ScanSample, ScanSnapshot,
    ScanTrace, SimulationLimits, SimulationProfile, Simulator, TimerState, compile_program,
};
mod metadata;
mod model;
mod project;
mod source;
pub use ir::*;

pub use error::GxwError;
pub use model::*;
pub use project::{
    FramingStatus, OpaqueRegion, ParseDiagnostic, ParsedProject, RawProgram, RawToken,
};
pub use source::{SourceInfo, SourceSpan};

use std::{fs::File, io::Read, path::Path, sync::Arc};

fn read_input(path: &Path, limits: &ReadLimits) -> Result<Vec<u8>, GxwError> {
    let file = File::open(path)?;
    limits.check("input bytes", file.metadata()?.len(), limits.max_file_bytes)?;
    let mut bytes = Vec::new();
    file.take(limits.max_file_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    limits.check("input bytes", bytes.len() as u64, limits.max_file_bytes)?;
    Ok(bytes)
}

pub fn load_csv(
    path: &Path,
    profile: DeviceProfile,
    limits: &ReadLimits,
) -> Result<InstructionProgram, GxwError> {
    parse_csv_bytes(read_input(path, limits)?.into(), profile, limits)
}

pub fn parse_csv_bytes(
    data: Arc<[u8]>,
    profile: DeviceProfile,
    limits: &ReadLimits,
) -> Result<InstructionProgram, GxwError> {
    formats::csv::parse(data, profile, limits)
}

pub fn decode_program(
    project: Arc<ParsedProject>,
    index: usize,
    profile: DeviceProfile,
) -> Result<InstructionProgram, GxwError> {
    formats::simple::decoder::decode(project, index, profile)
}

/// Inspect a file without opening it for writing. Limits also apply to growing files.
pub fn inspect_path(path: &Path, limits: &ReadLimits) -> Result<ProjectIndex, GxwError> {
    inspect_bytes(&read_input(path, limits)?, limits)
}

/// Inspect owned or borrowed bytes. The returned index owns all its metadata.
pub fn inspect_bytes(bytes: &[u8], limits: &ReadLimits) -> Result<ProjectIndex, GxwError> {
    read_project(bytes, limits, false).map(|(index, _)| index)
}

/// Load current POU candidates and raw records. No instruction semantics are inferred.
pub fn load_path(path: &Path, limits: &ReadLimits) -> Result<ParsedProject, GxwError> {
    parse_bytes(Arc::from(read_input(path, limits)?), limits)
}

pub fn parse_bytes(bytes: Arc<[u8]>, limits: &ReadLimits) -> Result<ParsedProject, GxwError> {
    let (index, streams) = read_project(&bytes, limits, true)?;
    project::parse(index, streams, limits)
}

type RetainedStreams = Vec<(StreamLocation, Arc<[u8]>)>;

fn read_project(
    bytes: &[u8],
    limits: &ReadLimits,
    retain_all: bool,
) -> Result<(ProjectIndex, RetainedStreams), GxwError> {
    limits.check("input bytes", bytes.len() as u64, limits.max_file_bytes)?;
    let mut budget = container::Budget::default();
    let (outer, payloads) = container::inspect(bytes, &[], limits, &mut budget, retain_all)?;
    let hdb = payloads
        .get(&vec!["_hdb".into()])
        .ok_or_else(|| GxwError::Unsupported {
            context: "outer CFB".into(),
            message: "missing _hdb stream".into(),
        })?;
    let (inner, inner_payloads) =
        container::inspect(hdb, &["_hdb".into()], limits, &mut budget, retain_all)?;
    let xml = payloads
        .get(&vec!["projectdatalist.xml".into()])
        .ok_or_else(|| GxwError::Unsupported {
            context: "outer CFB".into(),
            message: "missing projectdatalist.xml stream".into(),
        })?;
    let rows = metadata::rows(xml, "D_Projectdata", "projectdatalist.xml", limits)?;
    let mut diagnostics = vec![Diagnostic {
        code: "GXW_INSPECT_ONLY".into(), severity: "info".into(),
        message: "Container and metadata inspection only; instructions, CPU settings and execution are not decoded.".into(),
        location: None,
    }];
    let project_rows = match payloads.get(&vec!["projectlist.xml".into()]) {
        Some(data) => metadata::rows(data, "D_Project", "projectlist.xml", limits)?,
        None => {
            diagnostics.push(Diagnostic {
                code: "GXW_PROJECTLIST_MISSING".into(),
                severity: "warning".into(),
                message: "projectlist.xml is absent".into(),
                location: None,
            });
            Vec::new()
        }
    };
    let logical_objects = metadata::resolve(rows, &outer, &inner, &mut diagnostics);
    let streams = if retain_all {
        payloads
            .into_iter()
            .map(|(path, data)| {
                (
                    StreamLocation {
                        container: vec![],
                        path,
                    },
                    data,
                )
            })
            .chain(inner_payloads.into_iter().map(|(path, data)| {
                (
                    StreamLocation {
                        container: vec!["_hdb".into()],
                        path,
                    },
                    data,
                )
            }))
            .collect()
    } else {
        Vec::new()
    };
    Ok((
        ProjectIndex {
            schema_version: 1,
            size: bytes.len() as u64,
            sha256: container::digest(bytes),
            containers: vec![outer, inner],
            logical_objects,
            project_rows,
            diagnostics,
        },
        streams,
    ))
}
