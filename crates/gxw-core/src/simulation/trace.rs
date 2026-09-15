use super::{CompiledProgram, CounterState, TimerState, compile::X_COUNT, timers::Timer};
use crate::{DeviceKind, DeviceRef, GxwError, InstructionSpan, Opcode};
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug, Clone, Serialize)]
pub struct InstructionEvent {
    pub instruction: usize,
    pub opcode: Opcode,
    pub device: Option<DeviceRef>,
    pub source: InstructionSpan,
    pub accumulator: Option<bool>,
    pub read_value: Option<bool>,
    pub write_value: Option<bool>,
    /// Previous committed Y, before END refresh; absent for other devices.
    pub external_output: Option<bool>,
    pub time_ns: u64,
    pub timer: Option<TimerState>,
    pub counter: Option<CounterState>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ScanSnapshot {
    pub schema_version: u32,
    pub profile: super::SimulationProfile,
    pub execution_scope: String,
    pub scan_period_ns: u64,
    pub source_sha256: String,
    pub scan: u64,
    pub time_ns: u64,
    pub timer_phase_ns: u64,
    pub timing_model: &'static str,
    pub retention_policy: &'static str,
    pub running: bool,
    pub timers: BTreeMap<String, TimerState>,
    pub counters: BTreeMap<String, CounterState>,
    /// Referenced and explicitly updated devices; get() also reads other valid bits.
    pub devices: BTreeMap<String, bool>,
    pub outputs: BTreeMap<String, bool>,
    pub instruction_trace: Vec<InstructionEvent>,
    #[serde(skip)]
    pub(crate) image: Box<[bool]>,
    #[serde(skip)]
    pub(crate) output_image: [bool; super::compile::Y_COUNT],
    #[serde(skip)]
    pub(crate) compiled: Arc<CompiledProgram>,
}
impl ScanSnapshot {
    pub fn timer(&self, device: &str) -> Result<TimerState, GxwError> {
        let d = self.compiled.profile().parse_device(device)?;
        if d.device != DeviceKind::T {
            return Err(super::error(
                "GXW_SIM_STATE_TARGET",
                "timer() accepts only supported T devices",
                None,
            ));
        }
        Ok(self.timers.get(&d.name()).cloned().unwrap_or_else(|| {
            Timer::new(d.address, None).snapshot(self.time_ns, self.timer_phase_ns)
        }))
    }
    pub fn counter(&self, device: &str) -> Result<CounterState, GxwError> {
        let d = self.compiled.profile().parse_device(device)?;
        if d.device != DeviceKind::C {
            return Err(super::error(
                "GXW_SIM_STATE_TARGET",
                "counter() accepts only supported C devices",
                None,
            ));
        }
        Ok(self
            .counters
            .get(&d.name())
            .cloned()
            .unwrap_or_else(|| CounterState::new(d.address, None)))
    }
    pub fn get(&self, device: &str) -> Result<bool, GxwError> {
        let profile = self.compiled.profile();
        Ok(self.image[profile.slot(&profile.parse_device(device)?)?])
    }
    pub fn output(&self, device: &str) -> Result<bool, GxwError> {
        let profile = self.compiled.profile();
        let d = profile.parse_device(device)?;
        if d.device != DeviceKind::Y {
            return Err(super::error(
                "GXW_SIM_OUTPUT_TARGET",
                "output() accepts only Y devices",
                None,
            ));
        }
        Ok(self.output_image[profile.slot(&d)? - X_COUNT])
    }
    pub fn compiled(&self) -> &Arc<CompiledProgram> {
        &self.compiled
    }
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct ScanSample {
    pub scan: u64,
    pub time_ns: u64,
    /// Values align with ScanTrace.watch, after END output refresh.
    pub values: Vec<bool>,
    /// Numeric states for the T/C devices selected by watch, after the scan.
    pub timers: BTreeMap<String, TimerState>,
    pub counters: BTreeMap<String, CounterState>,
}
#[derive(Debug, Serialize)]
pub struct ScanTrace {
    pub schema_version: u32,
    pub profile: super::SimulationProfile,
    pub execution_scope: String,
    pub scan_period_ns: u64,
    pub timer_phase_ns: u64,
    pub timing_model: &'static str,
    pub retention_policy: &'static str,
    pub source_sha256: String,
    pub start_scan: u64,
    pub requested_scans: u64,
    pub completed_scans: u64,
    pub interrupted: bool,
    pub watch: Vec<String>,
    pub samples: Vec<ScanSample>,
    pub final_snapshot: ScanSnapshot,
}
impl ScanTrace {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}
