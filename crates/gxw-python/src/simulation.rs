use super::{
    analysis::PyDeviceRef,
    convert_error,
    instructions::{PyInstructionProgram, PyInstructionSpan},
};
use gxw_core as core;
use pyo3::{
    exceptions::{PyRuntimeError, PyTypeError},
    prelude::*,
    types::{PyBool, PyDict},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard, TryLockError},
};

#[pyclass(name = "TimerState", module = "gxwlib._core", frozen)]
struct PyTimerState {
    inner: core::TimerState,
}
#[pymethods]
impl PyTimerState {
    #[getter]
    fn device(&self) -> &str {
        &self.inner.device
    }
    #[getter]
    fn time_base_ns(&self) -> u64 {
        self.inner.time_base_ns
    }
    #[getter]
    fn evaluation(&self) -> &str {
        self.inner.evaluation
    }
    #[getter]
    fn retentive(&self) -> bool {
        self.inner.retentive
    }
    #[getter]
    fn preset(&self) -> Option<u16> {
        self.inner.preset
    }
    #[getter]
    fn value(&self) -> u16 {
        self.inner.value
    }
    #[getter]
    fn elapsed_ns(&self) -> u64 {
        self.inner.elapsed_ns
    }
    #[getter]
    fn phase_ns(&self) -> u64 {
        self.inner.phase_ns
    }
    #[getter]
    fn input(&self) -> bool {
        self.inner.input
    }
    #[getter]
    fn previous_input(&self) -> bool {
        self.inner.previous_input
    }
    #[getter]
    fn done(&self) -> bool {
        self.inner.done
    }
    #[getter]
    fn reset_active(&self) -> bool {
        self.inner.reset_active
    }
}
#[pyclass(name = "CounterState", module = "gxwlib._core", frozen)]
struct PyCounterState {
    inner: core::CounterState,
}
#[pymethods]
impl PyCounterState {
    #[getter]
    fn device(&self) -> &str {
        &self.inner.device
    }
    #[getter]
    fn retentive(&self) -> bool {
        self.inner.retentive
    }
    #[getter]
    fn preset(&self) -> Option<u16> {
        self.inner.preset
    }
    #[getter]
    fn value(&self) -> u16 {
        self.inner.value
    }
    #[getter]
    fn input(&self) -> bool {
        self.inner.input
    }
    #[getter]
    fn previous_input(&self) -> bool {
        self.inner.previous_input
    }
    #[getter]
    fn rising_edge(&self) -> bool {
        self.inner.rising_edge
    }
    #[getter]
    fn done(&self) -> bool {
        self.inner.done
    }
    #[getter]
    fn reset_active(&self) -> bool {
        self.inner.reset_active
    }
}
#[pyclass(name = "SimulationLimits", module = "gxwlib._core", frozen)]
struct PySimulationLimits {
    inner: core::SimulationLimits,
}
#[pymethods]
impl PySimulationLimits {
    #[new]
    #[pyo3(signature=(*,max_scans=100_000,max_operations=100_000_000,max_trace_values=1_000_000,max_events=100_000))]
    fn new(
        max_scans: u64,
        max_operations: u64,
        max_trace_values: usize,
        max_events: usize,
    ) -> Self {
        Self {
            inner: core::SimulationLimits {
                max_scans,
                max_operations,
                max_trace_values,
                max_events,
            },
        }
    }
    #[getter]
    fn max_scans(&self) -> u64 {
        self.inner.max_scans
    }
    #[getter]
    fn max_operations(&self) -> u64 {
        self.inner.max_operations
    }
    #[getter]
    fn max_trace_values(&self) -> usize {
        self.inner.max_trace_values
    }
    #[getter]
    fn max_events(&self) -> usize {
        self.inner.max_events
    }
}
#[pyclass(name = "CompiledInstruction", module = "gxwlib._core", frozen)]
struct PyCompiledInstruction {
    owner: Arc<core::CompiledProgram>,
    index: usize,
}
#[pymethods]
impl PyCompiledInstruction {
    #[getter]
    fn preset(&self) -> Option<u16> {
        self.owner.instructions()[self.index].preset
    }
    #[getter]
    fn instruction(&self) -> usize {
        self.owner.instructions()[self.index].instruction
    }
    #[getter]
    fn opcode(&self) -> &str {
        self.owner.instructions()[self.index].opcode.as_str()
    }
    #[getter]
    fn device(&self) -> Option<PyDeviceRef> {
        self.owner.instructions()[self.index]
            .device
            .clone()
            .map(|inner| PyDeviceRef { inner })
    }
    #[getter]
    fn source(&self) -> PyInstructionSpan {
        PyInstructionSpan {
            owner: self.owner.program().clone(),
            inner: self.owner.instructions()[self.index].source.clone(),
        }
    }
}
#[pyclass(name = "CompiledProgram", module = "gxwlib._core", frozen)]
struct PyCompiledProgram {
    inner: Arc<core::CompiledProgram>,
}
#[pymethods]
impl PyCompiledProgram {
    #[getter]
    fn schema_version(&self) -> u32 {
        self.inner.schema_version()
    }
    #[getter]
    fn profile(&self) -> &str {
        self.inner.profile().as_str()
    }
    #[getter]
    fn execution_scope(&self) -> &str {
        self.inner.execution_scope()
    }
    #[getter]
    fn source_sha256(&self) -> &str {
        self.inner.source_sha256()
    }
    #[getter]
    fn instructions(&self) -> Vec<PyCompiledInstruction> {
        (0..self.inner.instructions().len())
            .map(|index| PyCompiledInstruction {
                owner: self.inner.clone(),
                index,
            })
            .collect()
    }
    #[getter]
    fn devices(&self) -> Vec<PyDeviceRef> {
        self.inner
            .devices()
            .iter()
            .map(|d| PyDeviceRef { inner: d.clone() })
            .collect()
    }
    #[getter]
    fn program(&self) -> PyInstructionProgram {
        PyInstructionProgram {
            inner: self.inner.program().clone(),
        }
    }
    fn to_json(&self, py: Python<'_>) -> PyResult<String> {
        let p = self.inner.clone();
        py.detach(move || p.to_json())
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
}
#[derive(Clone)]
enum SnapshotOwner {
    Snapshot(Arc<core::ScanSnapshot>),
    Trace(Arc<core::ScanTrace>),
}
impl SnapshotOwner {
    fn inner(&self) -> &core::ScanSnapshot {
        match self {
            Self::Snapshot(s) => s,
            Self::Trace(t) => &t.final_snapshot,
        }
    }
}
#[pyclass(name = "InstructionEvent", module = "gxwlib._core", frozen)]
struct PyInstructionEvent {
    owner: SnapshotOwner,
    index: usize,
}
#[pymethods]
impl PyInstructionEvent {
    #[getter]
    fn time_ns(&self) -> u64 {
        self.owner.inner().instruction_trace[self.index].time_ns
    }
    #[getter]
    fn timer(&self) -> Option<PyTimerState> {
        self.owner.inner().instruction_trace[self.index]
            .timer
            .clone()
            .map(|inner| PyTimerState { inner })
    }
    #[getter]
    fn counter(&self) -> Option<PyCounterState> {
        self.owner.inner().instruction_trace[self.index]
            .counter
            .clone()
            .map(|inner| PyCounterState { inner })
    }
    #[getter]
    fn instruction(&self) -> usize {
        self.owner.inner().instruction_trace[self.index].instruction
    }
    #[getter]
    fn opcode(&self) -> &str {
        self.owner.inner().instruction_trace[self.index]
            .opcode
            .as_str()
    }
    #[getter]
    fn device(&self) -> Option<PyDeviceRef> {
        self.owner.inner().instruction_trace[self.index]
            .device
            .clone()
            .map(|inner| PyDeviceRef { inner })
    }
    #[getter]
    fn accumulator(&self) -> Option<bool> {
        self.owner.inner().instruction_trace[self.index].accumulator
    }
    #[getter]
    fn read_value(&self) -> Option<bool> {
        self.owner.inner().instruction_trace[self.index].read_value
    }
    #[getter]
    fn write_value(&self) -> Option<bool> {
        self.owner.inner().instruction_trace[self.index].write_value
    }
    #[getter]
    fn external_output(&self) -> Option<bool> {
        self.owner.inner().instruction_trace[self.index].external_output
    }
    #[getter]
    fn source(&self) -> PyInstructionSpan {
        PyInstructionSpan {
            owner: self.owner.inner().compiled().program().clone(),
            inner: self.owner.inner().instruction_trace[self.index]
                .source
                .clone(),
        }
    }
}
#[pyclass(name = "ScanSnapshot", module = "gxwlib._core", frozen)]
struct PyScanSnapshot {
    owner: SnapshotOwner,
}
#[pymethods]
impl PyScanSnapshot {
    #[getter]
    fn timer_phase_ns(&self) -> u64 {
        self.owner.inner().timer_phase_ns
    }
    #[getter]
    fn timing_model(&self) -> &str {
        self.owner.inner().timing_model
    }
    #[getter]
    fn retention_policy(&self) -> &str {
        self.owner.inner().retention_policy
    }
    #[getter]
    fn running(&self) -> bool {
        self.owner.inner().running
    }
    #[getter]
    fn timers(&self) -> BTreeMap<String, PyTimerState> {
        self.owner
            .inner()
            .timers
            .iter()
            .map(|(k, v)| (k.clone(), PyTimerState { inner: v.clone() }))
            .collect()
    }
    #[getter]
    fn counters(&self) -> BTreeMap<String, PyCounterState> {
        self.owner
            .inner()
            .counters
            .iter()
            .map(|(k, v)| (k.clone(), PyCounterState { inner: v.clone() }))
            .collect()
    }
    fn timer(&self, device: &str) -> PyResult<PyTimerState> {
        self.owner
            .inner()
            .timer(device)
            .map(|inner| PyTimerState { inner })
            .map_err(convert_error)
    }
    fn counter(&self, device: &str) -> PyResult<PyCounterState> {
        self.owner
            .inner()
            .counter(device)
            .map(|inner| PyCounterState { inner })
            .map_err(convert_error)
    }
    #[getter]
    fn profile(&self) -> &str {
        self.owner.inner().profile.as_str()
    }
    #[getter]
    fn execution_scope(&self) -> &str {
        &self.owner.inner().execution_scope
    }
    #[getter]
    fn scan_period_ns(&self) -> u64 {
        self.owner.inner().scan_period_ns
    }
    #[getter]
    fn schema_version(&self) -> u32 {
        self.owner.inner().schema_version
    }
    #[getter]
    fn source_sha256(&self) -> &str {
        &self.owner.inner().source_sha256
    }
    #[getter]
    fn scan(&self) -> u64 {
        self.owner.inner().scan
    }
    #[getter]
    fn time_ns(&self) -> u64 {
        self.owner.inner().time_ns
    }
    #[getter]
    fn devices(&self) -> BTreeMap<String, bool> {
        self.owner.inner().devices.clone()
    }
    #[getter]
    fn outputs(&self) -> BTreeMap<String, bool> {
        self.owner.inner().outputs.clone()
    }
    #[getter]
    fn instruction_trace(&self) -> Vec<PyInstructionEvent> {
        (0..self.owner.inner().instruction_trace.len())
            .map(|index| PyInstructionEvent {
                owner: self.owner.clone(),
                index,
            })
            .collect()
    }
    #[getter]
    fn compiled(&self) -> PyCompiledProgram {
        PyCompiledProgram {
            inner: self.owner.inner().compiled().clone(),
        }
    }
    fn get(&self, device: &str) -> PyResult<bool> {
        self.owner.inner().get(device).map_err(convert_error)
    }
    fn output(&self, device: &str) -> PyResult<bool> {
        self.owner.inner().output(device).map_err(convert_error)
    }
    fn to_json(&self, py: Python<'_>) -> PyResult<String> {
        let owner = self.owner.clone();
        py.detach(move || owner.inner().to_json())
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
}
#[pyclass(name = "ScanSample", module = "gxwlib._core", frozen)]
struct PyScanSample {
    owner: Arc<core::ScanTrace>,
    index: usize,
}
#[pymethods]
impl PyScanSample {
    #[getter]
    fn timers(&self) -> BTreeMap<String, PyTimerState> {
        self.owner.samples[self.index]
            .timers
            .iter()
            .map(|(k, v)| (k.clone(), PyTimerState { inner: v.clone() }))
            .collect()
    }
    #[getter]
    fn counters(&self) -> BTreeMap<String, PyCounterState> {
        self.owner.samples[self.index]
            .counters
            .iter()
            .map(|(k, v)| (k.clone(), PyCounterState { inner: v.clone() }))
            .collect()
    }
    #[getter]
    fn scan(&self) -> u64 {
        self.owner.samples[self.index].scan
    }
    #[getter]
    fn time_ns(&self) -> u64 {
        self.owner.samples[self.index].time_ns
    }
    #[getter]
    fn values(&self) -> Vec<bool> {
        self.owner.samples[self.index].values.clone()
    }
}
#[pyclass(name = "ScanTrace", module = "gxwlib._core", frozen)]
struct PyScanTrace {
    inner: Arc<core::ScanTrace>,
}
#[pymethods]
impl PyScanTrace {
    #[getter]
    fn timer_phase_ns(&self) -> u64 {
        self.inner.timer_phase_ns
    }
    #[getter]
    fn timing_model(&self) -> &str {
        self.inner.timing_model
    }
    #[getter]
    fn retention_policy(&self) -> &str {
        self.inner.retention_policy
    }
    #[getter]
    fn profile(&self) -> &str {
        self.inner.profile.as_str()
    }
    #[getter]
    fn execution_scope(&self) -> &str {
        &self.inner.execution_scope
    }
    #[getter]
    fn scan_period_ns(&self) -> u64 {
        self.inner.scan_period_ns
    }
    #[getter]
    fn schema_version(&self) -> u32 {
        self.inner.schema_version
    }
    #[getter]
    fn source_sha256(&self) -> &str {
        &self.inner.source_sha256
    }
    #[getter]
    fn start_scan(&self) -> u64 {
        self.inner.start_scan
    }
    #[getter]
    fn requested_scans(&self) -> u64 {
        self.inner.requested_scans
    }
    #[getter]
    fn completed_scans(&self) -> u64 {
        self.inner.completed_scans
    }
    #[getter]
    fn interrupted(&self) -> bool {
        self.inner.interrupted
    }
    #[getter]
    fn watch(&self) -> Vec<String> {
        self.inner.watch.clone()
    }
    #[getter]
    fn samples(&self) -> Vec<PyScanSample> {
        (0..self.inner.samples.len())
            .map(|index| PyScanSample {
                owner: self.inner.clone(),
                index,
            })
            .collect()
    }
    #[getter]
    fn final_snapshot(&self) -> PyScanSnapshot {
        PyScanSnapshot {
            owner: SnapshotOwner::Trace(self.inner.clone()),
        }
    }
    fn to_json(&self, py: Python<'_>) -> PyResult<String> {
        let owner = self.inner.clone();
        py.detach(move || owner.to_json())
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
}
fn assignments(values: Option<&Bound<'_, PyDict>>) -> PyResult<Vec<(String, bool)>> {
    let Some(values) = values else {
        return Ok(Vec::new());
    };
    values
        .iter()
        .map(|(key, value)| {
            if !value.is_instance_of::<PyBool>() {
                return Err(PyTypeError::new_err(
                    "Device values must be bool (True/False)",
                ));
            }
            Ok((key.extract::<String>()?, value.extract::<bool>()?))
        })
        .collect()
}
fn lock(sim: &Mutex<core::Simulator>) -> PyResult<MutexGuard<'_, core::Simulator>> {
    sim.try_lock().map_err(|e| {
        convert_error(core::GxwError::Simulation {
            code: if matches!(e, TryLockError::WouldBlock) {
                "GXW_SIM_BUSY"
            } else {
                "GXW_SIM_POISONED"
            }
            .into(),
            message: if matches!(e, TryLockError::WouldBlock) {
                "Simulator is in use by another operation"
            } else {
                "Simulator lock is poisoned"
            }
            .into(),
            instruction: None,
        })
    })
}
#[pyclass(name = "Simulator", module = "gxwlib._core", frozen)]
struct PySimulator {
    inner: Arc<Mutex<core::Simulator>>,
    compiled: Arc<core::CompiledProgram>,
}
#[pymethods]
impl PySimulator {
    #[new]
    #[pyo3(signature=(compiled,*,scan_period_ns,initial_state=None,limits=None,timer_phase_ns=0))]
    fn new(
        py: Python<'_>,
        compiled: PyRef<'_, PyCompiledProgram>,
        scan_period_ns: u64,
        initial_state: Option<&Bound<'_, PyDict>>,
        limits: Option<PyRef<'_, PySimulationLimits>>,
        timer_phase_ns: u64,
    ) -> PyResult<Self> {
        let compiled = compiled.inner.clone();
        let owner = compiled.clone();
        let initial_state = assignments(initial_state)?;
        let limits = limits.map(|v| v.inner.clone()).unwrap_or_default();
        let sim = py
            .detach(move || {
                core::Simulator::with_timer_phase(
                    owner,
                    scan_period_ns,
                    &initial_state,
                    limits,
                    timer_phase_ns,
                )
            })
            .map_err(convert_error)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(sim)),
            compiled,
        })
    }
    #[getter]
    fn compiled(&self) -> PyCompiledProgram {
        PyCompiledProgram {
            inner: self.compiled.clone(),
        }
    }
    #[getter]
    fn scan_period_ns(&self) -> PyResult<u64> {
        Ok(lock(&self.inner)?.scan_period_ns())
    }
    #[getter]
    fn timer_phase_ns(&self) -> PyResult<u64> {
        Ok(lock(&self.inner)?.timer_phase_ns())
    }
    #[getter]
    fn running(&self) -> PyResult<bool> {
        Ok(lock(&self.inner)?.running())
    }
    fn set_scan_period_ns(&self, py: Python<'_>, period_ns: u64) -> PyResult<()> {
        let owner = self.inner.clone();
        py.detach(move || {
            lock(&owner)?
                .set_scan_period_ns(period_ns)
                .map_err(convert_error)
        })
    }
    fn stop(&self, py: Python<'_>) -> PyResult<()> {
        let owner = self.inner.clone();
        py.detach(move || {
            lock(&owner)?.stop();
            Ok(())
        })
    }
    fn start(&self, py: Python<'_>) -> PyResult<()> {
        let owner = self.inner.clone();
        py.detach(move || {
            lock(&owner)?.start();
            Ok(())
        })
    }
    fn advance_stopped(&self, py: Python<'_>, elapsed_ns: u64) -> PyResult<()> {
        let owner = self.inner.clone();
        py.detach(move || {
            lock(&owner)?
                .advance_stopped(elapsed_ns)
                .map_err(convert_error)
        })
    }
    #[getter]
    fn busy(&self) -> bool {
        self.inner.try_lock().is_err()
    }
    #[getter]
    fn scan_count(&self) -> PyResult<u64> {
        Ok(lock(&self.inner)?.scan_count())
    }
    #[getter]
    fn time_ns(&self) -> PyResult<u64> {
        Ok(lock(&self.inner)?.time_ns())
    }
    fn set_inputs(&self, py: Python<'_>, values: &Bound<'_, PyDict>) -> PyResult<()> {
        let values = assignments(Some(values))?;
        let owner = self.inner.clone();
        py.detach(move || lock(&owner)?.set_inputs(&values).map_err(convert_error))
    }
    fn set_devices(&self, py: Python<'_>, values: &Bound<'_, PyDict>) -> PyResult<()> {
        let values = assignments(Some(values))?;
        let owner = self.inner.clone();
        py.detach(move || lock(&owner)?.set_devices(&values).map_err(convert_error))
    }
    #[pyo3(signature=(*,initial_state=None))]
    fn reset(&self, py: Python<'_>, initial_state: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        let values = assignments(initial_state)?;
        let owner = self.inner.clone();
        py.detach(move || lock(&owner)?.reset(&values).map_err(convert_error))
    }
    fn snapshot(&self, py: Python<'_>) -> PyResult<PyScanSnapshot> {
        let owner = self.inner.clone();
        let snapshot = py.detach(move || Ok::<_, PyErr>(lock(&owner)?.snapshot()))?;
        Ok(PyScanSnapshot {
            owner: SnapshotOwner::Snapshot(Arc::new(snapshot)),
        })
    }
    #[pyo3(signature=(*,trace_instructions=false))]
    fn step(&self, py: Python<'_>, trace_instructions: bool) -> PyResult<PyScanSnapshot> {
        let owner = self.inner.clone();
        let snapshot = py.detach(move || {
            lock(&owner)?
                .step(trace_instructions)
                .map_err(convert_error)
        })?;
        Ok(PyScanSnapshot {
            owner: SnapshotOwner::Snapshot(Arc::new(snapshot)),
        })
    }
    #[pyo3(signature=(scans,*,watch=None))]
    fn run(&self, py: Python<'_>, scans: u64, watch: Option<Vec<String>>) -> PyResult<PyScanTrace> {
        let owner = self.inner.clone();
        let (trace, signal) = py.detach(move || {
            let mut sim = lock(&owner)?;
            let mut signal = None;
            let trace = sim
                .run_interruptible(scans, watch.as_deref(), || {
                    if let Err(e) = Python::attach(|py| py.check_signals()) {
                        signal = Some(e);
                        true
                    } else {
                        false
                    }
                })
                .map_err(convert_error)?;
            Ok::<_, PyErr>((trace, signal))
        })?;
        let trace = PyScanTrace {
            inner: Arc::new(trace),
        };
        if let Some(err) = signal {
            err.value(py)
                .setattr("completed_scans", trace.inner.completed_scans)?;
            err.value(py).setattr("trace", Py::new(py, trace)?)?;
            return Err(err);
        }
        Ok(trace)
    }
}
#[pyfunction]
#[pyo3(signature=(program,*,profile,max_instructions=100_000))]
fn compile_program(
    py: Python<'_>,
    program: PyRef<'_, PyInstructionProgram>,
    profile: &str,
    max_instructions: usize,
) -> PyResult<PyCompiledProgram> {
    let profile = core::SimulationProfile::parse(profile).map_err(convert_error)?;
    let owner = program.inner.clone();
    let compiled = py
        .detach(move || core::compile_program(owner, profile, max_instructions))
        .map_err(convert_error)?;
    Ok(PyCompiledProgram {
        inner: Arc::new(compiled),
    })
}
pub(super) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyTimerState>()?;
    m.add_class::<PyCounterState>()?;
    m.add_class::<PySimulationLimits>()?;
    m.add_class::<PyCompiledInstruction>()?;
    m.add_class::<PyCompiledProgram>()?;
    m.add_class::<PyInstructionEvent>()?;
    m.add_class::<PyScanSnapshot>()?;
    m.add_class::<PyScanSample>()?;
    m.add_class::<PyScanTrace>()?;
    m.add_class::<PySimulator>()?;
    m.add_function(wrap_pyfunction!(compile_program, m)?)?;
    Ok(())
}
