//! Syntactic references and bounded FX ladder diagnostics, never a safety proof.
use crate::{DeviceKind, GxwError, InstructionProgram, InstructionSpan, Opcode, OperandValue};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
pub mod diff;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct DeviceRef {
    pub device: DeviceKind,
    pub address: u32,
}
impl DeviceRef {
    pub fn parse(text: &str) -> Result<Self, GxwError> {
        match crate::ir::text_operand(text) {
            OperandValue::Device {
                device, address, ..
            } => Ok(Self { device, address }),
            _ => Err(GxwError::format(
                "external write device",
                format!("unsupported FX device: {text}"),
            )),
        }
    }
    pub fn name(&self) -> String {
        if matches!(self.device, DeviceKind::X | DeviceKind::Y) {
            format!("{:?}{:o}", self.device, self.address)
        } else {
            format!("{:?}{}", self.device, self.address)
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnalysisOptions {
    pub external_writes: Vec<DeviceRef>,
    pub inputs_external: bool,
    pub max_instructions: usize,
    pub max_block_depth: usize,
    pub max_mps_depth: usize,
}
impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            external_writes: Vec::new(),
            inputs_external: true,
            max_instructions: 100_000,
            max_block_depth: 8,
            max_mps_depth: 11,
        }
    }
}
pub(crate) fn limit(name: &str, actual: usize, max: usize) -> Result<(), GxwError> {
    if actual > max {
        Err(GxwError::ResourceLimit {
            resource: name.into(),
            actual: actual as u64,
            limit: max as u64,
        })
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub code: String,
    pub severity: String,
    pub message: String,
    pub instruction: Option<usize>,
    pub related_instructions: Vec<usize>,
    pub source: Option<InstructionSpan>,
}
impl Finding {
    pub(crate) fn at(
        p: &InstructionProgram,
        index: Option<usize>,
        code: &str,
        severity: &str,
        message: &str,
    ) -> Self {
        Self {
            code: code.into(),
            severity: severity.into(),
            message: message.into(),
            instruction: index,
            related_instructions: Vec::new(),
            source: index
                .and_then(|i| p.instructions.get(i))
                .map(|i| i.source.clone()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessMode {
    Read,
    Write,
    Unknown,
}
#[derive(Debug, Clone, Serialize)]
pub struct DeviceAccess {
    pub device: DeviceRef,
    pub instruction: usize,
    pub operand: usize,
    pub mode: AccessMode,
    pub write_kind: Option<Opcode>,
    pub source: InstructionSpan,
}
#[derive(Debug, Clone, Serialize)]
pub struct DeviceUsage {
    pub device: DeviceRef,
    pub external_write: bool,
    /// Indices into AnalysisReport.accesses; preserves repeated operands.
    pub reads: Vec<usize>,
    pub writes: Vec<usize>,
    pub unknown: Vec<usize>,
}
#[derive(Debug, Serialize)]
pub struct AnalysisReport {
    pub schema_version: u32,
    pub source_sha256: String,
    pub complete: bool,
    pub accesses: Vec<DeviceAccess>,
    pub devices: Vec<DeviceUsage>,
    pub diagnostics: Vec<Finding>,
    #[serde(skip)]
    pub program: Arc<InstructionProgram>,
}
impl AnalysisReport {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

pub fn analyze(
    program: Arc<InstructionProgram>,
    options: &AnalysisOptions,
) -> Result<AnalysisReport, GxwError> {
    limit(
        "analysis instructions",
        program.instructions.len(),
        options.max_instructions,
    )?;
    let graph = crate::ladder::build(program.clone(), options)?;
    let mut report = AnalysisReport {
        schema_version: 1,
        source_sha256: program.source_sha256.clone(),
        complete: graph.complete,
        accesses: Vec::new(),
        devices: Vec::new(),
        diagnostics: graph.diagnostics,
        program: program.clone(),
    };
    let external: BTreeSet<_> = options.external_writes.iter().cloned().collect();
    let mut devices = BTreeMap::<DeviceRef, DeviceUsage>::new();
    for (index, ins) in program.instructions.iter().enumerate() {
        let valid = ins.supported && crate::ir::valid_instruction(ins);
        for (operand_index, operand) in ins.operands.iter().enumerate() {
            let OperandValue::Device {
                device, address, ..
            } = operand.value
            else {
                if operand.value == OperandValue::Unknown {
                    let mut finding = Finding::at(
                        &program,
                        Some(index),
                        "GXW_OPERAND_UNRESOLVED",
                        "warning",
                        "Operand is unresolved; its device accesses are unknown",
                    );
                    finding.source = Some(operand.source.clone());
                    report.diagnostics.push(finding);
                }
                continue;
            };
            let mode = if valid {
                match ins.opcode {
                    Some(
                        Opcode::Ld
                        | Opcode::Ldi
                        | Opcode::And
                        | Opcode::Ani
                        | Opcode::Or
                        | Opcode::Ori,
                    ) => AccessMode::Read,
                    Some(Opcode::Out | Opcode::Set | Opcode::Rst) if operand_index == 0 => {
                        AccessMode::Write
                    }
                    Some(Opcode::Out) => AccessMode::Read, // T/C preset device, not the target.
                    _ => AccessMode::Unknown,
                }
            } else {
                AccessMode::Unknown
            };
            let device = DeviceRef { device, address };
            let usage = devices
                .entry(device.clone())
                .or_insert_with(|| DeviceUsage {
                    external_write: external.contains(&device)
                        || (options.inputs_external && device.device == DeviceKind::X),
                    device: device.clone(),
                    reads: Vec::new(),
                    writes: Vec::new(),
                    unknown: Vec::new(),
                });
            let slot = report.accesses.len();
            match mode {
                AccessMode::Read => usage.reads.push(slot),
                AccessMode::Write => usage.writes.push(slot),
                AccessMode::Unknown => usage.unknown.push(slot),
            }
            report.accesses.push(DeviceAccess {
                device,
                instruction: index,
                operand: operand_index,
                mode,
                write_kind: if mode == AccessMode::Write {
                    ins.opcode
                } else {
                    None
                },
                source: operand.source.clone(),
            });
        }
        if !valid {
            report.diagnostics.push(Finding::at(
                &program,
                Some(index),
                "GXW_ACCESS_UNRESOLVED",
                "warning",
                "Read/write effects of this instruction are not resolved",
            ));
        }
    }
    report.devices = devices.into_values().collect();
    for usage in &report.devices {
        let sites: Vec<_> = usage.writes.iter().map(|i| &report.accesses[*i]).collect();
        let set_reset_pair = sites.len() == 2
            && sites.iter().any(|a| a.write_kind == Some(Opcode::Set))
            && sites.iter().any(|a| a.write_kind == Some(Opcode::Rst));
        if sites.len() > 1 && !set_reset_pair {
            let mut f = Finding::at(
                &program,
                Some(sites[0].instruction),
                "GXW_MULTIPLE_WRITES",
                "warning",
                &format!(
                    "{} has multiple local writes; behavior depends on instruction order",
                    usage.device.name()
                ),
            );
            f.related_instructions = sites.iter().map(|a| a.instruction).collect();
            report.diagnostics.push(f);
        }
        if !usage.reads.is_empty() && usage.writes.is_empty() && !usage.external_write {
            report.diagnostics.push(Finding::at(&program, Some(report.accesses[usage.reads[0]].instruction), "GXW_NO_KNOWN_LOCAL_WRITE", "info",
                &format!("{} has no resolved local write; retained state, external writers and unresolved instructions may supply it", usage.device.name())));
        }
    }
    Ok(report)
}
