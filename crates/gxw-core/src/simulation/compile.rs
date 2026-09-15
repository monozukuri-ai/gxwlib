use super::{error, profiles::fx3g};
use crate::{
    AnalysisOptions, DeviceKind, DeviceRef, GxwError, InstructionProgram, InstructionSpan, Opcode,
    OperandValue, analysis::limit, build_ladder,
};
use serde::Serialize;
use std::{collections::BTreeSet, sync::Arc};

pub(crate) const X_COUNT: usize = 128;
pub(crate) const Y_COUNT: usize = 128;
pub(crate) const M_COUNT: usize = 7680;
pub(crate) const T_START: usize = X_COUNT + Y_COUNT + M_COUNT;
pub(crate) const C_START: usize = T_START + 320;
pub(crate) const MEMORY_SIZE: usize = C_START + 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SimulationProfile {
    Fx3g,
}
impl SimulationProfile {
    pub fn parse(name: &str) -> Result<Self, GxwError> {
        match name {
            "fx3g" => Ok(Self::Fx3g),
            _ => Err(error(
                "GXW_SIM_PROFILE_UNSUPPORTED",
                format!("Unsupported simulation profile: {name}"),
                None,
            )),
        }
    }
    pub fn as_str(self) -> &'static str {
        "fx3g"
    }
    pub fn parse_device(self, name: &str) -> Result<DeviceRef, GxwError> {
        let d = DeviceRef::parse(name).map_err(|_| {
            error(
                "GXW_SIM_DEVICE_INVALID",
                format!("Invalid FX bit device: {name}"),
                None,
            )
        })?;
        self.slot(&d)?;
        Ok(d)
    }
    pub(crate) fn slot(self, d: &DeviceRef) -> Result<usize, GxwError> {
        let a = d.address as usize;
        match d.device {
            DeviceKind::X if a < X_COUNT => Ok(a),
            DeviceKind::Y if a < Y_COUNT => Ok(X_COUNT + a),
            DeviceKind::M if a < M_COUNT => Ok(X_COUNT + Y_COUNT + a),
            DeviceKind::T if fx3g::timer(d.address).is_some() => Ok(T_START + a),
            DeviceKind::C if a < 200 => Ok(C_START + a),
            _ => Err(error(
                "GXW_SIM_DEVICE_UNSUPPORTED",
                format!(
                    "{} is outside the FX3G simulation subset (X/Y0..177 octal, M0..7679, T0..191/T200..245/T250..319, C0..199 decimal); routine/interrupt timers, special devices and words are unsupported",
                    d.name()
                ),
                None,
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CompiledInstruction {
    pub instruction: usize,
    pub opcode: Opcode,
    pub device: Option<DeviceRef>,
    pub preset: Option<u16>,
    pub source: InstructionSpan,
    #[serde(skip)]
    pub(crate) slot: usize,
}
/// No public constructor or mutable operations: only the compiler can create
/// executable instructions. The source remains owned and immutable under Arc.
#[derive(Debug, Serialize)]
pub struct CompiledProgram {
    schema_version: u32,
    profile: SimulationProfile,
    execution_scope: &'static str,
    source_sha256: String,
    instructions: Vec<CompiledInstruction>,
    devices: Vec<DeviceRef>,
    #[serde(skip)]
    program: Arc<InstructionProgram>,
}
impl CompiledProgram {
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }
    pub fn profile(&self) -> SimulationProfile {
        self.profile
    }
    pub fn execution_scope(&self) -> &str {
        self.execution_scope
    }
    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }
    pub fn instructions(&self) -> &[CompiledInstruction] {
        &self.instructions
    }
    pub fn devices(&self) -> &[DeviceRef] {
        &self.devices
    }
    pub fn program(&self) -> &Arc<InstructionProgram> {
        &self.program
    }
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

pub fn compile_program(
    program: Arc<InstructionProgram>,
    profile: SimulationProfile,
    max_instructions: usize,
) -> Result<CompiledProgram, GxwError> {
    limit(
        "simulation compiler instruction limit",
        max_instructions,
        1_000_000,
    )?;
    limit(
        "simulation instructions",
        program.instructions.len(),
        max_instructions,
    )?;
    if !program.complete || !program.opaque_regions.is_empty() {
        return Err(error(
            "GXW_SIM_INCOMPLETE",
            "Instruction program is incomplete; unsupported code and opaque regions cannot execute",
            None,
        ));
    }
    // Check the source program itself, never a caller-supplied display graph.
    let mut devices = BTreeSet::new();
    let mut instructions = Vec::with_capacity(program.instructions.len());
    let mut coils = BTreeSet::new();
    let mut resets = BTreeSet::new();
    for (index, ins) in program.instructions.iter().enumerate() {
        if ins.ordinal != index || !ins.supported || !crate::ir::valid_instruction(ins) {
            return Err(error(
                "GXW_SIM_INSTRUCTION_UNSUPPORTED",
                "Unvalidated instruction or operand",
                Some(index),
            ));
        }
        if ins.source.length == 0 || program.source_bytes(&ins.source).is_none() {
            return Err(error(
                "GXW_SIM_SOURCE_INVALID",
                "Instruction has no valid source range",
                Some(index),
            ));
        }
        let (device, slot) = if let Some(operand) = ins.operands.first() {
            let OperandValue::Device {
                device, address, ..
            } = operand.value
            else {
                return Err(error(
                    "GXW_SIM_DEVICE_UNSUPPORTED",
                    "Expected a resolved bit device",
                    Some(index),
                ));
            };
            let d = DeviceRef { device, address };
            let slot = profile
                .slot(&d)
                .map_err(|e| error("GXW_SIM_DEVICE_UNSUPPORTED", e.to_string(), Some(index)))?;
            devices.insert(d.clone());
            (Some(d), slot)
        } else {
            (None, 0)
        };
        let mut preset = None;
        if let Some(d) = &device
            && matches!(d.device, DeviceKind::T | DeviceKind::C)
        {
            if ins.opcode == Some(Opcode::Out) {
                let Some(OperandValue::Constant {
                    value, radix: 10, ..
                }) = ins.operands.get(1).map(|o| &o.value)
                else {
                    return Err(error(
                        "GXW_SIM_PRESET_UNSUPPORTED",
                        "T/C OUT requires a literal K0..K32767 preset; D presets are not executable",
                        Some(index),
                    ));
                };
                preset = Some(*value as u16);
                if !coils.insert(d.clone()) {
                    return Err(error(
                        "GXW_SIM_MULTIPLE_COIL",
                        "Only one OUT per timer/counter is supported",
                        Some(index),
                    ));
                }
            } else if ins.opcode == Some(Opcode::Rst) && !resets.insert(d.clone()) {
                return Err(error(
                    "GXW_SIM_MULTIPLE_RESET",
                    "Only one RST per timer/counter is supported",
                    Some(index),
                ));
            }
        }
        instructions.push(CompiledInstruction {
            instruction: index,
            opcode: ins.opcode.expect("validated opcode"),
            device,
            preset,
            source: ins.source.clone(),
            slot,
        });
    }
    let end_valid = instructions.last().is_some_and(|i| i.opcode == Opcode::End)
        && instructions
            .iter()
            .filter(|i| i.opcode == Opcode::End)
            .count()
            == 1;
    if !end_valid {
        return Err(error(
            "GXW_SIM_END_INVALID",
            "Expected exactly one final END",
            None,
        ));
    }
    let graph = build_ladder(
        program.clone(),
        &AnalysisOptions {
            max_instructions,
            ..AnalysisOptions::default()
        },
    )?;
    if !graph.complete {
        let f = graph
            .diagnostics
            .first()
            .expect("incomplete structure has diagnostic");
        return Err(error(
            "GXW_SIM_STRUCTURE_INVALID",
            format!("{}: {}", f.code, f.message),
            f.instruction,
        ));
    }
    Ok(CompiledProgram {
        schema_version: 2,
        profile,
        execution_scope: "isolated_single_program",
        source_sha256: program.source_sha256.clone(),
        instructions,
        devices: devices.into_iter().collect(),
        program,
    })
}
