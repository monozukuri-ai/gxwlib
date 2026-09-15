use super::{
    convert_error,
    instructions::{PyInstructionProgram, PyInstructionSpan},
};
use gxw_core as core;
use pyo3::{exceptions::PyRuntimeError, prelude::*};
use std::sync::Arc;
#[pyclass(name = "AnalysisOptions", module = "gxwlib._core", frozen)]
struct PyAnalysisOptions {
    inner: core::AnalysisOptions,
}
#[pymethods]
impl PyAnalysisOptions {
    #[new]
    #[pyo3(signature=(*, external_writes=None, inputs_external=true, max_instructions=100_000, max_block_depth=8, max_mps_depth=11))]
    fn new(
        external_writes: Option<Vec<String>>,
        inputs_external: bool,
        max_instructions: usize,
        max_block_depth: usize,
        max_mps_depth: usize,
    ) -> PyResult<Self> {
        let external_writes = external_writes
            .unwrap_or_default()
            .iter()
            .map(|s| core::DeviceRef::parse(s))
            .collect::<Result<Vec<_>, _>>()
            .map_err(convert_error)?;
        Ok(Self {
            inner: core::AnalysisOptions {
                external_writes,
                inputs_external,
                max_instructions,
                max_block_depth,
                max_mps_depth,
            },
        })
    }
    #[getter]
    fn external_writes(&self) -> Vec<String> {
        self.inner
            .external_writes
            .iter()
            .map(|d| d.name())
            .collect()
    }
    #[getter]
    fn inputs_external(&self) -> bool {
        self.inner.inputs_external
    }
    #[getter]
    fn max_instructions(&self) -> usize {
        self.inner.max_instructions
    }
    #[getter]
    fn max_block_depth(&self) -> usize {
        self.inner.max_block_depth
    }
    #[getter]
    fn max_mps_depth(&self) -> usize {
        self.inner.max_mps_depth
    }
}
#[pyclass(name = "DeviceRef", module = "gxwlib._core", frozen)]
pub(super) struct PyDeviceRef {
    pub(super) inner: core::DeviceRef,
}
#[pymethods]
impl PyDeviceRef {
    #[getter]
    fn name(&self) -> String {
        self.inner.name()
    }
    #[getter]
    fn device(&self) -> String {
        format!("{:?}", self.inner.device)
    }
    #[getter]
    fn address(&self) -> u32 {
        self.inner.address
    }
}
#[pyclass(name = "Finding", module = "gxwlib._core", frozen)]
struct PyFinding {
    inner: core::Finding,
    owner: Arc<core::InstructionProgram>,
}
#[pymethods]
impl PyFinding {
    #[getter]
    fn code(&self) -> &str {
        &self.inner.code
    }
    #[getter]
    fn severity(&self) -> &str {
        &self.inner.severity
    }
    #[getter]
    fn message(&self) -> &str {
        &self.inner.message
    }
    #[getter]
    fn instruction(&self) -> Option<usize> {
        self.inner.instruction
    }
    #[getter]
    fn related_instructions(&self) -> Vec<usize> {
        self.inner.related_instructions.clone()
    }
    #[getter]
    fn source(&self) -> Option<PyInstructionSpan> {
        self.inner.source.as_ref().map(|s| PyInstructionSpan {
            owner: self.owner.clone(),
            inner: s.clone(),
        })
    }
}
#[pyclass(name = "DeviceAccess", module = "gxwlib._core", frozen)]
struct PyDeviceAccess {
    inner: core::DeviceAccess,
    owner: Arc<core::InstructionProgram>,
}
#[pymethods]
impl PyDeviceAccess {
    #[getter]
    fn device(&self) -> PyDeviceRef {
        PyDeviceRef {
            inner: self.inner.device.clone(),
        }
    }
    #[getter]
    fn instruction(&self) -> usize {
        self.inner.instruction
    }
    #[getter]
    fn operand(&self) -> usize {
        self.inner.operand
    }
    #[getter]
    fn mode(&self) -> &str {
        match self.inner.mode {
            core::AccessMode::Read => "read",
            core::AccessMode::Write => "write",
            core::AccessMode::Unknown => "unknown",
        }
    }
    #[getter]
    fn write_kind(&self) -> Option<&str> {
        self.inner.write_kind.map(|op| op.as_str())
    }
    #[getter]
    fn source(&self) -> PyInstructionSpan {
        PyInstructionSpan {
            owner: self.owner.clone(),
            inner: self.inner.source.clone(),
        }
    }
}
#[pyclass(name = "DeviceUsage", module = "gxwlib._core", frozen)]
struct PyDeviceUsage {
    inner: core::DeviceUsage,
}
#[pymethods]
impl PyDeviceUsage {
    #[getter]
    fn device(&self) -> PyDeviceRef {
        PyDeviceRef {
            inner: self.inner.device.clone(),
        }
    }
    #[getter]
    fn external_write(&self) -> bool {
        self.inner.external_write
    }
    #[getter]
    fn reads(&self) -> Vec<usize> {
        self.inner.reads.clone()
    }
    #[getter]
    fn writes(&self) -> Vec<usize> {
        self.inner.writes.clone()
    }
    #[getter]
    fn unknown(&self) -> Vec<usize> {
        self.inner.unknown.clone()
    }
}
#[pyclass(name = "Condition", module = "gxwlib._core", frozen)]
struct PyCondition {
    inner: core::Condition,
}
#[pymethods]
impl PyCondition {
    #[getter]
    fn id(&self) -> usize {
        self.inner.id
    }
    #[getter]
    fn instruction(&self) -> usize {
        self.inner.instruction
    }
    #[getter]
    fn kind(&self) -> &str {
        match self.inner.kind {
            core::ConditionKind::Contact => "contact",
            core::ConditionKind::And => "and",
            core::ConditionKind::Or => "or",
        }
    }
    #[getter]
    fn inputs(&self) -> Vec<usize> {
        self.inner.inputs.clone()
    }
    #[getter]
    fn inverted(&self) -> bool {
        self.inner.inverted
    }
}
#[pyclass(name = "LadderOutput", module = "gxwlib._core", frozen)]
struct PyLadderOutput {
    inner: core::LadderOutput,
}
#[pymethods]
impl PyLadderOutput {
    #[getter]
    fn instruction(&self) -> usize {
        self.inner.instruction
    }
    #[getter]
    fn condition(&self) -> usize {
        self.inner.condition
    }
}
#[pyclass(name = "LadderControl", module = "gxwlib._core", frozen)]
struct PyLadderControl {
    inner: core::LadderControl,
}
#[pymethods]
impl PyLadderControl {
    #[getter]
    fn instruction(&self) -> usize {
        self.inner.instruction
    }
    #[getter]
    fn condition(&self) -> Option<usize> {
        self.inner.condition
    }
}
#[pyclass(name = "SvgElement", module = "gxwlib._core", frozen)]
struct PySvgElement {
    inner: core::SvgElement,
    owner: Arc<core::InstructionProgram>,
}
#[pymethods]
impl PySvgElement {
    #[getter]
    fn id(&self) -> &str {
        &self.inner.id
    }
    #[getter]
    fn kind(&self) -> &str {
        &self.inner.kind
    }
    #[getter]
    fn label(&self) -> &str {
        &self.inner.label
    }
    #[getter]
    fn instruction(&self) -> usize {
        self.inner.instruction
    }
    #[getter]
    fn x(&self) -> usize {
        self.inner.x
    }
    #[getter]
    fn y(&self) -> usize {
        self.inner.y
    }
    #[getter]
    fn width(&self) -> usize {
        self.inner.width
    }
    #[getter]
    fn height(&self) -> usize {
        self.inner.height
    }
    #[getter]
    fn inverted(&self) -> bool {
        self.inner.inverted
    }
    #[getter]
    fn source(&self) -> PyInstructionSpan {
        PyInstructionSpan {
            owner: self.owner.clone(),
            inner: self.inner.source.clone(),
        }
    }
}
#[pyclass(name = "AnalysisReport", module = "gxwlib._core", frozen)]
struct PyAnalysisReport {
    inner: Arc<core::AnalysisReport>,
}
#[pymethods]
impl PyAnalysisReport {
    #[getter]
    fn schema_version(&self) -> u32 {
        self.inner.schema_version
    }
    #[getter]
    fn source_sha256(&self) -> &str {
        &self.inner.source_sha256
    }
    #[getter]
    fn complete(&self) -> bool {
        self.inner.complete
    }
    #[getter]
    fn accesses(&self) -> Vec<PyDeviceAccess> {
        self.inner
            .accesses
            .iter()
            .map(|inner| PyDeviceAccess {
                inner: inner.clone(),
                owner: self.inner.program.clone(),
            })
            .collect()
    }
    #[getter]
    fn devices(&self) -> Vec<PyDeviceUsage> {
        self.inner
            .devices
            .iter()
            .map(|inner| PyDeviceUsage {
                inner: inner.clone(),
            })
            .collect()
    }
    #[getter]
    fn diagnostics(&self) -> Vec<PyFinding> {
        self.inner
            .diagnostics
            .iter()
            .map(|inner| PyFinding {
                inner: inner.clone(),
                owner: self.inner.program.clone(),
            })
            .collect()
    }
    #[getter]
    fn program(&self) -> PyInstructionProgram {
        PyInstructionProgram {
            inner: self.inner.program.clone(),
        }
    }
    fn to_json(&self, py: Python<'_>) -> PyResult<String> {
        let owner = self.inner.clone();
        py.detach(move || owner.to_json())
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
}
#[pyclass(name = "LadderGraph", module = "gxwlib._core", frozen)]
struct PyLadderGraph {
    inner: Arc<core::LadderGraph>,
}
#[pymethods]
impl PyLadderGraph {
    #[getter]
    fn schema_version(&self) -> u32 {
        self.inner.schema_version
    }
    #[getter]
    fn source_sha256(&self) -> &str {
        &self.inner.source_sha256
    }
    #[getter]
    fn complete(&self) -> bool {
        self.inner.complete
    }
    #[getter]
    fn processed_instructions(&self) -> usize {
        self.inner.processed_instructions
    }
    #[getter]
    fn conditions(&self) -> Vec<PyCondition> {
        self.inner
            .conditions
            .iter()
            .map(|inner| PyCondition {
                inner: inner.clone(),
            })
            .collect()
    }
    #[getter]
    fn outputs(&self) -> Vec<PyLadderOutput> {
        self.inner
            .outputs
            .iter()
            .map(|inner| PyLadderOutput {
                inner: inner.clone(),
            })
            .collect()
    }
    #[getter]
    fn controls(&self) -> Vec<PyLadderControl> {
        self.inner
            .controls
            .iter()
            .map(|inner| PyLadderControl {
                inner: inner.clone(),
            })
            .collect()
    }
    #[getter]
    fn diagnostics(&self) -> Vec<PyFinding> {
        self.inner
            .diagnostics
            .iter()
            .map(|inner| PyFinding {
                inner: inner.clone(),
                owner: self.inner.program.clone(),
            })
            .collect()
    }
    #[getter]
    fn program(&self) -> PyInstructionProgram {
        PyInstructionProgram {
            inner: self.inner.program.clone(),
        }
    }
    fn to_json(&self, py: Python<'_>) -> PyResult<String> {
        let owner = self.inner.clone();
        py.detach(move || owner.to_json())
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
}
#[pyclass(name = "SvgDocument", module = "gxwlib._core", frozen)]
struct PySvgDocument {
    inner: Arc<core::SvgDocument>,
}
#[pymethods]
impl PySvgDocument {
    #[getter]
    fn schema_version(&self) -> u32 {
        self.inner.schema_version
    }
    #[getter]
    fn source_sha256(&self) -> &str {
        &self.inner.source_sha256
    }
    #[getter]
    fn complete(&self) -> bool {
        self.inner.complete
    }
    #[getter]
    fn layout(&self) -> &str {
        &self.inner.layout
    }
    #[getter]
    fn width(&self) -> usize {
        self.inner.width
    }
    #[getter]
    fn height(&self) -> usize {
        self.inner.height
    }
    #[getter]
    fn svg(&self) -> &str {
        &self.inner.svg
    }
    #[getter]
    fn elements(&self) -> Vec<PySvgElement> {
        self.inner
            .elements
            .iter()
            .map(|inner| PySvgElement {
                inner: inner.clone(),
                owner: self.inner.graph.program.clone(),
            })
            .collect()
    }
    #[getter]
    fn program(&self) -> PyInstructionProgram {
        PyInstructionProgram {
            inner: self.inner.graph.program.clone(),
        }
    }
    fn to_json(&self, py: Python<'_>) -> PyResult<String> {
        let owner = self.inner.clone();
        py.detach(move || owner.to_json())
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
    #[getter]
    fn graph(&self) -> PyLadderGraph {
        PyLadderGraph {
            inner: self.inner.graph.clone(),
        }
    }
    fn _repr_svg_(&self) -> &str {
        &self.inner.svg
    }
    fn to_html(&self, py: Python<'_>) -> PyResult<String> {
        let owner = self.inner.clone();
        py.detach(move || owner.to_html()).map_err(convert_error)
    }
}
#[pyclass(name = "DiffHunk", module = "gxwlib._core", frozen)]
struct PyDiffHunk {
    owner: Arc<core::ProgramDiff>,
    index: usize,
}
#[pymethods]
impl PyDiffHunk {
    #[getter]
    fn kind(&self) -> &str {
        match self.owner.hunks[self.index].kind {
            core::DiffKind::Equal => "equal",
            core::DiffKind::Insert => "insert",
            core::DiffKind::Delete => "delete",
            core::DiffKind::Replace => "replace",
        }
    }
    #[getter]
    fn left_start(&self) -> usize {
        self.owner.hunks[self.index].left_start
    }
    #[getter]
    fn left_count(&self) -> usize {
        self.owner.hunks[self.index].left_count
    }
    #[getter]
    fn right_start(&self) -> usize {
        self.owner.hunks[self.index].right_start
    }
    #[getter]
    fn right_count(&self) -> usize {
        self.owner.hunks[self.index].right_count
    }
    #[getter]
    fn left_sources(&self) -> Vec<PyInstructionSpan> {
        let h = &self.owner.hunks[self.index];
        self.owner.left.instructions[h.left_start..h.left_start + h.left_count]
            .iter()
            .map(|i| PyInstructionSpan {
                owner: self.owner.left.clone(),
                inner: i.source.clone(),
            })
            .collect()
    }
    #[getter]
    fn right_sources(&self) -> Vec<PyInstructionSpan> {
        let h = &self.owner.hunks[self.index];
        self.owner.right.instructions[h.right_start..h.right_start + h.right_count]
            .iter()
            .map(|i| PyInstructionSpan {
                owner: self.owner.right.clone(),
                inner: i.source.clone(),
            })
            .collect()
    }
}
#[pyclass(name = "ProgramDiff", module = "gxwlib._core", frozen)]
struct PyProgramDiff {
    inner: Arc<core::ProgramDiff>,
}
#[pymethods]
impl PyProgramDiff {
    #[getter]
    fn schema_version(&self) -> u32 {
        self.inner.schema_version
    }
    #[getter]
    fn left_sha256(&self) -> &str {
        &self.inner.left_sha256
    }
    #[getter]
    fn right_sha256(&self) -> &str {
        &self.inner.right_sha256
    }
    #[getter]
    fn complete(&self) -> bool {
        self.inner.complete
    }
    #[getter]
    fn different(&self) -> bool {
        self.inner.different
    }
    #[getter]
    fn opaque_changed(&self) -> bool {
        self.inner.opaque_changed
    }
    #[getter]
    fn hunks(&self) -> Vec<PyDiffHunk> {
        (0..self.inner.hunks.len())
            .map(|index| PyDiffHunk {
                owner: self.inner.clone(),
                index,
            })
            .collect()
    }
    #[getter]
    fn left_program(&self) -> PyInstructionProgram {
        PyInstructionProgram {
            inner: self.inner.left.clone(),
        }
    }
    #[getter]
    fn right_program(&self) -> PyInstructionProgram {
        PyInstructionProgram {
            inner: self.inner.right.clone(),
        }
    }
    fn to_json(&self, py: Python<'_>) -> PyResult<String> {
        let owner = self.inner.clone();
        py.detach(move || owner.to_json())
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
}
#[pyfunction]
#[pyo3(signature=(program,*,options=None))]
fn analyze(
    py: Python<'_>,
    program: PyRef<'_, PyInstructionProgram>,
    options: Option<PyRef<'_, PyAnalysisOptions>>,
) -> PyResult<PyAnalysisReport> {
    let owner = program.inner.clone();
    let options = options.map(|o| o.inner.clone()).unwrap_or_default();
    let inner = py
        .detach(move || core::analyze(owner, &options))
        .map_err(convert_error)?;
    Ok(PyAnalysisReport {
        inner: Arc::new(inner),
    })
}
#[pyfunction]
#[pyo3(signature=(program,*,options=None))]
fn build_ladder(
    py: Python<'_>,
    program: PyRef<'_, PyInstructionProgram>,
    options: Option<PyRef<'_, PyAnalysisOptions>>,
) -> PyResult<PyLadderGraph> {
    let owner = program.inner.clone();
    let options = options.map(|o| o.inner.clone()).unwrap_or_default();
    let inner = py
        .detach(move || core::build_ladder(owner, &options))
        .map_err(convert_error)?;
    Ok(PyLadderGraph {
        inner: Arc::new(inner),
    })
}
#[pyfunction]
#[pyo3(signature=(program,*,options=None,max_elements=20_000,max_output_bytes=16_777_216))]
fn render_ladder(
    py: Python<'_>,
    program: PyRef<'_, PyInstructionProgram>,
    options: Option<PyRef<'_, PyAnalysisOptions>>,
    max_elements: usize,
    max_output_bytes: usize,
) -> PyResult<PySvgDocument> {
    let owner = program.inner.clone();
    let options = options.map(|o| o.inner.clone()).unwrap_or_default();
    let inner = py
        .detach(move || {
            let graph = core::build_ladder(owner, &options)?;
            core::render_svg(
                Arc::new(graph),
                &core::RenderOptions {
                    max_elements,
                    max_output_bytes,
                },
            )
        })
        .map_err(convert_error)?;
    Ok(PySvgDocument {
        inner: Arc::new(inner),
    })
}
#[pyfunction]
#[pyo3(signature=(left,right,*,max_cells=1_000_000))]
fn diff_programs(
    py: Python<'_>,
    left: PyRef<'_, PyInstructionProgram>,
    right: PyRef<'_, PyInstructionProgram>,
    max_cells: usize,
) -> PyResult<PyProgramDiff> {
    let left = left.inner.clone();
    let right = right.inner.clone();
    let inner = py
        .detach(move || core::diff_programs(left, right, max_cells))
        .map_err(convert_error)?;
    Ok(PyProgramDiff {
        inner: Arc::new(inner),
    })
}
pub(super) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyAnalysisOptions>()?;
    m.add_class::<PyDeviceRef>()?;
    m.add_class::<PyFinding>()?;
    m.add_class::<PyDeviceAccess>()?;
    m.add_class::<PyDeviceUsage>()?;
    m.add_class::<PyCondition>()?;
    m.add_class::<PyLadderOutput>()?;
    m.add_class::<PyLadderControl>()?;
    m.add_class::<PySvgElement>()?;
    m.add_class::<PyAnalysisReport>()?;
    m.add_class::<PyLadderGraph>()?;
    m.add_class::<PySvgDocument>()?;
    m.add_class::<PyDiffHunk>()?;
    m.add_class::<PyProgramDiff>()?;
    m.add_function(wrap_pyfunction!(analyze, m)?)?;
    m.add_function(wrap_pyfunction!(build_ladder, m)?)?;
    m.add_function(wrap_pyfunction!(render_ladder, m)?)?;
    m.add_function(wrap_pyfunction!(diff_programs, m)?)?;
    Ok(())
}
