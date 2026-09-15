use super::{
    CompiledProgram, CounterState, InstructionEvent, ScanSample, ScanSnapshot, ScanTrace,
    TimerState,
    compile::{C_START, M_COUNT, T_START, X_COUNT, Y_COUNT},
    error,
    memory::{Memory, patch},
    profiles::fx3g,
    timers::Timer,
};
use crate::{DeviceKind, DeviceRef, GxwError, Opcode, analysis::limit};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Debug, Clone)]
pub struct SimulationLimits {
    pub max_scans: u64,
    pub max_operations: u64,
    pub max_trace_values: usize,
    pub max_events: usize,
}
impl Default for SimulationLimits {
    fn default() -> Self {
        Self {
            max_scans: 100_000,
            max_operations: 100_000_000,
            max_trace_values: 1_000_000,
            max_events: 100_000,
        }
    }
}
impl SimulationLimits {
    fn validate(&self) -> Result<(), GxwError> {
        cap("scan limit", self.max_scans, 1_000_000)?;
        cap("operation limit", self.max_operations, 1_000_000_000)?;
        limit("trace value limit", self.max_trace_values, 10_000_000)?;
        limit("instruction event limit", self.max_events, 1_000_000)
    }
}
fn cap(resource: &str, actual: u64, maximum: u64) -> Result<(), GxwError> {
    if actual > maximum {
        Err(GxwError::ResourceLimit {
            resource: resource.into(),
            actual,
            limit: maximum,
        })
    } else {
        Ok(())
    }
}

#[derive(Debug)]
pub struct Simulator {
    compiled: Arc<CompiledProgram>,
    scan_period_ns: u64,
    limits: SimulationLimits,
    memory: Memory,
    tracked: BTreeSet<DeviceRef>,
    scan: u64,
    time_ns: u64,
    timer_phase_ns: u64,
    running: bool,
    timers: BTreeMap<usize, Timer>,
    counters: BTreeMap<usize, CounterState>,
}
impl Simulator {
    pub fn new(
        compiled: Arc<CompiledProgram>,
        scan_period_ns: u64,
        initial_state: &[(String, bool)],
        limits: SimulationLimits,
    ) -> Result<Self, GxwError> {
        Self::with_timer_phase(compiled, scan_period_ns, initial_state, limits, 0)
    }
    pub fn with_timer_phase(
        compiled: Arc<CompiledProgram>,
        scan_period_ns: u64,
        initial_state: &[(String, bool)],
        limits: SimulationLimits,
        timer_phase_ns: u64,
    ) -> Result<Self, GxwError> {
        if timer_phase_ns >= fx3g::MAX_PHASE_NS {
            return Err(error(
                "GXW_SIM_PHASE_INVALID",
                "timer_phase_ns must be in 0..100000000",
                None,
            ));
        }
        if scan_period_ns == 0 {
            return Err(error(
                "GXW_SIM_PERIOD_INVALID",
                "scan_period_ns must be positive",
                None,
            ));
        }
        limits.validate()?;
        let mut timers = BTreeMap::new();
        let mut counters = BTreeMap::new();
        for d in compiled.devices() {
            let slot = compiled.profile().slot(d)?;
            match d.device {
                DeviceKind::T => {
                    timers.insert(slot, Timer::new(d.address, None));
                }
                DeviceKind::C => {
                    counters.insert(slot, CounterState::new(d.address, None));
                }
                _ => {}
            }
        }
        for ins in compiled.instructions() {
            if let Some(preset) = ins.preset {
                if let Some(t) = timers.get_mut(&ins.slot) {
                    t.state.preset = Some(preset);
                }
                if let Some(c) = counters.get_mut(&ins.slot) {
                    c.preset = Some(preset);
                }
            }
        }
        let mut s = Self {
            tracked: compiled.devices().iter().cloned().collect(),
            compiled,
            scan_period_ns,
            limits,
            memory: Memory::default(),
            scan: 0,
            time_ns: 0,
            timer_phase_ns,
            running: true,
            timers,
            counters,
        };
        s.set_devices(initial_state)?;
        Ok(s)
    }
    pub fn compiled(&self) -> &Arc<CompiledProgram> {
        &self.compiled
    }
    pub fn scan_period_ns(&self) -> u64 {
        self.scan_period_ns
    }
    pub fn scan_count(&self) -> u64 {
        self.scan
    }
    pub fn time_ns(&self) -> u64 {
        self.time_ns
    }
    pub fn timer_phase_ns(&self) -> u64 {
        self.timer_phase_ns
    }
    pub fn running(&self) -> bool {
        self.running
    }
    pub fn set_scan_period_ns(&mut self, period: u64) -> Result<(), GxwError> {
        if period == 0 {
            return Err(error(
                "GXW_SIM_PERIOD_INVALID",
                "scan_period_ns must be positive",
                None,
            ));
        }
        self.scan_period_ns = period;
        Ok(())
    }
    /// FX3G factory retention only, with M8033 and parameter overrides absent.
    pub fn stop(&mut self) {
        if !self.running {
            return;
        }
        self.running = false;
        self.memory.image[..X_COUNT + Y_COUNT].fill(false);
        self.memory.outputs.fill(false);
        for a in 0..M_COUNT {
            if !fx3g::relay_retentive(a as u32) {
                self.memory.image[X_COUNT + Y_COUNT + a] = false;
            }
        }
        for (slot, t) in &mut self.timers {
            t.stop(self.time_ns);
            self.memory.image[*slot] = t.state.done;
        }
        for (slot, c) in &mut self.counters {
            c.stop();
            self.memory.image[*slot] = c.done;
        }
    }
    pub fn start(&mut self) {
        if self.running {
            return;
        }
        for t in self.timers.values_mut() {
            t.rebase(self.time_ns);
        }
        self.running = true;
    }
    /// Advance only the oscillator while stopped; no scan, input latch or count.
    pub fn advance_stopped(&mut self, elapsed_ns: u64) -> Result<(), GxwError> {
        if self.running {
            return Err(error(
                "GXW_SIM_NOT_STOPPED",
                "advance_stopped requires stop()",
                None,
            ));
        }
        let next = self
            .time_ns
            .checked_add(elapsed_ns)
            .ok_or_else(|| error("GXW_SIM_TIME_OVERFLOW", "Stopped time would overflow", None))?;
        self.time_ns = next;
        for t in self.timers.values_mut() {
            t.rebase(next);
        }
        Ok(())
    }
    pub fn set_inputs(&mut self, values: &[(String, bool)]) -> Result<(), GxwError> {
        let patch = patch(self.compiled.profile(), values, true)?;
        for (d, slot, value) in patch {
            self.memory.pending_inputs[slot] = value;
            self.tracked.insert(d);
        }
        Ok(())
    }
    pub fn set_devices(&mut self, values: &[(String, bool)]) -> Result<(), GxwError> {
        let patch = patch(self.compiled.profile(), values, false)?;
        for (d, slot, value) in patch {
            self.memory.image[slot] = value;
            self.tracked.insert(d);
        }
        Ok(())
    }
    /// Explicit initialization, separate from STOP/RUN or power cycling.
    pub fn reset(&mut self, initial_state: &[(String, bool)]) -> Result<(), GxwError> {
        let replacement = Self::with_timer_phase(
            self.compiled.clone(),
            self.scan_period_ns,
            initial_state,
            self.limits.clone(),
            self.timer_phase_ns,
        )?;
        *self = replacement;
        Ok(())
    }
    pub fn snapshot(&self) -> ScanSnapshot {
        self.make_snapshot(Vec::new())
    }
    fn make_snapshot(&self, events: Vec<InstructionEvent>) -> ScanSnapshot {
        let mut devices = BTreeMap::new();
        let mut outputs = BTreeMap::new();
        for d in &self.tracked {
            let slot = self
                .compiled
                .profile()
                .slot(d)
                .expect("validated tracked bit");
            devices.insert(d.name(), self.memory.image[slot]);
            if d.device == DeviceKind::Y {
                outputs.insert(d.name(), self.memory.outputs[slot - X_COUNT]);
            }
        }
        ScanSnapshot {
            schema_version: 2,
            profile: self.compiled.profile(),
            execution_scope: self.compiled.execution_scope().into(),
            scan_period_ns: self.scan_period_ns,
            source_sha256: self.compiled.source_sha256().into(),
            scan: self.scan,
            time_ns: self.time_ns,
            timer_phase_ns: self.timer_phase_ns,
            timing_model: fx3g::TIMING_MODEL,
            retention_policy: fx3g::RETENTION_POLICY,
            running: self.running,
            timers: self
                .timers
                .values()
                .map(|t| {
                    (
                        t.state.device.clone(),
                        t.snapshot(self.time_ns, self.timer_phase_ns),
                    )
                })
                .collect(),
            counters: self
                .counters
                .values()
                .map(|c| (c.device.clone(), c.clone()))
                .collect(),
            devices,
            outputs,
            instruction_trace: events,
            image: self.memory.image.clone(),
            output_image: self.memory.outputs,
            compiled: self.compiled.clone(),
        }
    }
    fn preflight(&self, scans: u64) -> Result<(), GxwError> {
        if !self.running {
            return Err(error(
                "GXW_SIM_STOPPED",
                "step/run require start() after stop()",
                None,
            ));
        }
        cap("requested scans", scans, self.limits.max_scans)?;
        let operations = scans
            .checked_mul(self.compiled.instructions().len() as u64)
            .ok_or_else(|| {
                error(
                    "GXW_SIM_OPERATION_OVERFLOW",
                    "Instruction count overflow",
                    None,
                )
            })?;
        cap(
            "scan instruction operations",
            operations,
            self.limits.max_operations,
        )?;
        let valid = scans
            .checked_mul(self.scan_period_ns)
            .and_then(|delta| self.time_ns.checked_add(delta));
        if valid.is_none() || self.scan.checked_add(scans).is_none() {
            return Err(error(
                "GXW_SIM_TIME_OVERFLOW",
                "Scan counter or virtual time would overflow; no scans executed",
                None,
            ));
        }
        Ok(())
    }
    pub fn step(&mut self, trace_instructions: bool) -> Result<ScanSnapshot, GxwError> {
        self.preflight(1)?;
        if trace_instructions {
            limit(
                "instruction trace events",
                self.compiled.instructions().len(),
                self.limits.max_events,
            )?;
        }
        let events = self.execute_scan(trace_instructions);
        Ok(self.make_snapshot(events))
    }
    pub fn run(&mut self, scans: u64, watch: Option<&[String]>) -> Result<ScanTrace, GxwError> {
        self.run_interruptible(scans, watch, || false)
    }
    /// Cancellation is checked at complete scan boundaries, at most 256 scans
    /// or roughly 16K operations per chunk. The last completed scan is retained.
    pub fn run_interruptible(
        &mut self,
        scans: u64,
        watch: Option<&[String]>,
        mut interrupted: impl FnMut() -> bool,
    ) -> Result<ScanTrace, GxwError> {
        self.preflight(scans)?;
        let watch: Vec<String> = watch
            .map(|v| v.to_vec())
            .unwrap_or_else(|| self.tracked.iter().map(DeviceRef::name).collect());
        let mut seen = BTreeSet::new();
        let mut slots = Vec::new();
        let mut names = Vec::new();
        for name in watch {
            let d = self.compiled.profile().parse_device(&name)?;
            if !seen.insert(d.clone()) {
                return Err(error(
                    "GXW_SIM_DUPLICATE_DEVICE",
                    format!("Duplicate watch: {}", d.name()),
                    None,
                ));
            }
            slots.push(self.compiled.profile().slot(&d)?);
            names.push(d.name());
        }
        // A T/C watch retains a structured numeric state as well as its bit.
        let weight: u64 = slots
            .iter()
            .map(|s| if *s >= T_START { 16 } else { 1 })
            .sum();
        let values = scans
            .checked_mul(weight)
            .ok_or_else(|| error("GXW_SIM_TRACE_OVERFLOW", "Trace size overflow", None))?;
        cap("trace values", values, self.limits.max_trace_values as u64)?;
        let start_scan = self.scan;
        let mut samples = Vec::with_capacity(scans as usize);
        let chunk = (16_384 / self.compiled.instructions().len()).clamp(1, 256) as u64;
        let mut cancelled = interrupted();
        while (samples.len() as u64) < scans && !cancelled {
            let end = (samples.len() as u64 + chunk).min(scans);
            while (samples.len() as u64) < end {
                self.execute_scan(false);
                samples.push(ScanSample {
                    scan: self.scan,
                    time_ns: self.time_ns,
                    values: slots.iter().map(|slot| self.memory.image[*slot]).collect(),
                    timers: slots
                        .iter()
                        .filter(|s| **s >= T_START && **s < C_START)
                        .map(|s| {
                            let address = (*s - T_START) as u32;
                            let t = self.timer_state(address);
                            (t.device.clone(), t)
                        })
                        .collect(),
                    counters: slots
                        .iter()
                        .filter(|s| **s >= C_START)
                        .map(|s| {
                            let c = self.counter_state((*s - C_START) as u32);
                            (c.device.clone(), c)
                        })
                        .collect(),
                });
            }
            cancelled = interrupted();
        }
        Ok(ScanTrace {
            schema_version: 2,
            profile: self.compiled.profile(),
            execution_scope: self.compiled.execution_scope().into(),
            scan_period_ns: self.scan_period_ns,
            timer_phase_ns: self.timer_phase_ns,
            timing_model: fx3g::TIMING_MODEL,
            retention_policy: fx3g::RETENTION_POLICY,
            source_sha256: self.compiled.source_sha256().into(),
            start_scan,
            requested_scans: scans,
            completed_scans: samples.len() as u64,
            interrupted: cancelled,
            watch: names,
            samples,
            final_snapshot: self.snapshot(),
        })
    }
    fn execute_scan(&mut self, trace: bool) -> Vec<InstructionEvent> {
        self.memory.latch_inputs();
        for c in self.counters.values_mut() {
            c.rising_edge = false;
        }
        let mut current = None;
        let mut blocks = [false; 7];
        let mut block_depth = 0;
        let mut saved = [false; 11];
        let mut saved_depth = 0;
        let mut events = if trace {
            Vec::with_capacity(self.compiled.instructions().len())
        } else {
            Vec::new()
        };
        for ins in self.compiled.instructions() {
            let (mut read_value, mut write_value) = (None, None);
            match ins.opcode {
                Opcode::Ld | Opcode::Ldi => {
                    if let Some(previous) = current {
                        if block_depth == 7 {
                            blocks.copy_within(1..7, 0);
                            block_depth = 6;
                        }
                        blocks[block_depth] = previous;
                        block_depth += 1;
                    }
                    let value = self.memory.image[ins.slot];
                    read_value = Some(value);
                    current = Some(if ins.opcode == Opcode::Ldi {
                        !value
                    } else {
                        value
                    });
                }
                Opcode::And | Opcode::Ani | Opcode::Or | Opcode::Ori => {
                    let raw = self.memory.image[ins.slot];
                    read_value = Some(raw);
                    let value = if matches!(ins.opcode, Opcode::Ani | Opcode::Ori) {
                        !raw
                    } else {
                        raw
                    };
                    let old = current.expect("compiled logic predecessor");
                    current = Some(if matches!(ins.opcode, Opcode::And | Opcode::Ani) {
                        old & value
                    } else {
                        old | value
                    });
                }
                Opcode::Anb | Opcode::Orb => {
                    block_depth -= 1;
                    let value = blocks[block_depth];
                    let old = current.unwrap();
                    current = Some(if ins.opcode == Opcode::Anb {
                        old & value
                    } else {
                        old | value
                    });
                }
                Opcode::Mps => {
                    saved[saved_depth] = current.unwrap();
                    saved_depth += 1;
                }
                Opcode::Mrd => {
                    current = Some(saved[saved_depth - 1]);
                }
                Opcode::Mpp => {
                    saved_depth -= 1;
                    current = Some(saved[saved_depth]);
                }
                Opcode::Out => {
                    let value = current.unwrap();
                    if let Some(t) = self.timers.get_mut(&ins.slot) {
                        t.drive(value, self.time_ns, self.timer_phase_ns);
                        self.memory.image[ins.slot] = t.state.done;
                    } else if let Some(c) = self.counters.get_mut(&ins.slot) {
                        c.drive(value);
                        self.memory.image[ins.slot] = c.done;
                    } else {
                        self.memory.image[ins.slot] = value;
                    }
                    write_value = Some(self.memory.image[ins.slot]);
                }
                Opcode::Set | Opcode::Rst => {
                    if ins.opcode == Opcode::Rst {
                        if let Some(t) = self.timers.get_mut(&ins.slot) {
                            t.reset_coil(current.unwrap(), self.time_ns);
                        }
                        if let Some(c) = self.counters.get_mut(&ins.slot) {
                            c.reset_coil(current.unwrap());
                        }
                    }
                    if current.unwrap() {
                        let value = ins.opcode == Opcode::Set;
                        self.memory.image[ins.slot] = value;
                        write_value = Some(value);
                    }
                }
                Opcode::End => {}
            }
            if trace {
                let external_output = ins
                    .device
                    .as_ref()
                    .filter(|d| d.device == DeviceKind::Y)
                    .map(|_| self.memory.outputs[ins.slot - X_COUNT]);
                events.push(InstructionEvent {
                    instruction: ins.instruction,
                    opcode: ins.opcode,
                    device: ins.device.clone(),
                    source: ins.source.clone(),
                    accumulator: current,
                    read_value,
                    write_value,
                    external_output,
                    time_ns: self.time_ns,
                    timer: self
                        .timers
                        .get(&ins.slot)
                        .filter(|_| ins.device.is_some())
                        .map(|t| t.snapshot(self.time_ns, self.timer_phase_ns)),
                    counter: self
                        .counters
                        .get(&ins.slot)
                        .filter(|_| ins.device.is_some())
                        .cloned(),
                });
            }
        }
        self.memory.commit_outputs();
        self.scan += 1;
        self.time_ns += self.scan_period_ns;
        events
    }
    fn timer_state(&self, address: u32) -> TimerState {
        self.timers
            .get(&(T_START + address as usize))
            .map(|t| t.snapshot(self.time_ns, self.timer_phase_ns))
            .unwrap_or_else(|| {
                Timer::new(address, None).snapshot(self.time_ns, self.timer_phase_ns)
            })
    }
    fn counter_state(&self, address: u32) -> CounterState {
        self.counters
            .get(&(C_START + address as usize))
            .cloned()
            .unwrap_or_else(|| CounterState::new(address, None))
    }
}
