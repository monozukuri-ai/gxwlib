use super::{
    convert_error,
    instructions::{PyInstructionProgram, profile},
    raw::{PyOpaqueRegion, PyParseDiagnostic, PyParsedProject, PySourceSpan, span},
};
use gxw_core::{self as core, structured as s};
use pyo3::{exceptions::PyRuntimeError, prelude::*};
use std::sync::Arc;
#[pyclass(name = "StructuredLimits", module = "gxwlib._core", frozen)]
pub(super) struct PyStructuredLimits {
    inner: s::StructuredLimits,
}
#[pymethods]
impl PyStructuredLimits {
    #[new]
    #[pyo3(signature=(*,max_blocks=4096,max_records=100_000,max_ports=200_000,max_string_units=65_536,max_declarations=65_536,max_connection_checks=5_000_000))]
    fn new(
        max_blocks: u64,
        max_records: u64,
        max_ports: u64,
        max_string_units: u64,
        max_declarations: u64,
        max_connection_checks: u64,
    ) -> Self {
        Self {
            inner: s::StructuredLimits {
                max_blocks,
                max_records,
                max_ports,
                max_string_units,
                max_declarations,
                max_connection_checks,
            },
        }
    }
    #[getter]
    fn max_blocks(&self) -> u64 {
        self.inner.max_blocks
    }
    #[getter]
    fn max_records(&self) -> u64 {
        self.inner.max_records
    }
    #[getter]
    fn max_ports(&self) -> u64 {
        self.inner.max_ports
    }
    #[getter]
    fn max_string_units(&self) -> u64 {
        self.inner.max_string_units
    }
    #[getter]
    fn max_declarations(&self) -> u64 {
        self.inner.max_declarations
    }
    #[getter]
    fn max_connection_checks(&self) -> u64 {
        self.inner.max_connection_checks
    }
}
#[pyclass(name = "StructuredProgram", module = "gxwlib._core", frozen)]
pub(super) struct PyStructuredProgram {
    pub(super) owner: Arc<s::StructuredProgram>,
}
impl PyStructuredProgram {
    fn inner(&self) -> &s::StructuredProgram {
        &self.owner
    }
}
#[pymethods]
impl PyStructuredProgram {
    #[getter]
    fn schema_version(&self) -> u32 {
        self.inner().schema_version
    }
    #[getter]
    fn source_sha256(&self) -> String {
        self.inner().source_sha256.clone()
    }
    #[getter]
    fn logical_name(&self) -> Option<String> {
        self.inner().logical_name.clone()
    }
    #[getter]
    fn logical_index(&self) -> usize {
        self.inner().logical_index
    }
    #[getter]
    fn profile(&self) -> String {
        self.inner().profile.clone()
    }
    #[getter]
    fn source(&self) -> PySourceSpan {
        span(self.owner.project(), &self.inner().source)
    }
    #[getter]
    fn header(&self) -> PySourceSpan {
        span(self.owner.project(), &self.inner().header)
    }
    #[getter]
    fn trailer(&self) -> PySourceSpan {
        span(self.owner.project(), &self.inner().trailer)
    }
    #[getter]
    fn structure_complete(&self) -> bool {
        self.inner().structure_complete
    }
    #[getter]
    fn semantic_status(&self) -> String {
        self.inner().semantic_status.clone()
    }
    #[getter]
    fn blocks(&self) -> Vec<PyStructuredBlock> {
        (0..self.inner().blocks.len())
            .map(|block| PyStructuredBlock {
                owner: self.owner.clone(),
                block,
            })
            .collect()
    }
    #[getter]
    fn diagnostics(&self) -> Vec<PyParseDiagnostic> {
        self.inner()
            .diagnostics
            .iter()
            .cloned()
            .map(|inner| PyParseDiagnostic {
                owner: self.owner.project().clone(),
                inner,
            })
            .collect()
    }
    fn to_json(&self, py: Python<'_>) -> PyResult<String> {
        let owner = self.owner.clone();
        py.detach(move || owner.to_json())
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
}
#[pyclass(name = "StructuredBlock", module = "gxwlib._core", frozen)]
pub(super) struct PyStructuredBlock {
    pub(super) owner: Arc<s::StructuredProgram>,
    block: usize,
}
impl PyStructuredBlock {
    fn inner(&self) -> &s::StructuredBlock {
        &self.owner.blocks[self.block]
    }
}
#[pymethods]
impl PyStructuredBlock {
    #[getter]
    fn index(&self) -> usize {
        self.inner().index
    }
    #[getter]
    fn source(&self) -> PySourceSpan {
        span(self.owner.project(), &self.inner().source)
    }
    #[getter]
    fn header(&self) -> PySourceSpan {
        span(self.owner.project(), &self.inner().header)
    }
    #[getter]
    fn canvas_height(&self) -> u32 {
        self.inner().canvas_height
    }
    #[getter]
    fn record_count(&self) -> u32 {
        self.inner().record_count
    }
    #[getter]
    fn connectivity_complete(&self) -> bool {
        self.inner().connectivity_complete
    }
    #[getter]
    fn nodes(&self) -> Vec<PyStructuredNode> {
        (0..self.inner().nodes.len())
            .map(|node| PyStructuredNode {
                owner: self.owner.clone(),
                block: self.block,
                node,
            })
            .collect()
    }
    #[getter]
    fn wires(&self) -> Vec<PyStructuredWire> {
        (0..self.inner().wires.len())
            .map(|wire| PyStructuredWire {
                owner: self.owner.clone(),
                block: self.block,
                wire,
            })
            .collect()
    }
    #[getter]
    fn nets(&self) -> Vec<PyStructuredNet> {
        (0..self.inner().nets.len())
            .map(|net| PyStructuredNet {
                owner: self.owner.clone(),
                block: self.block,
                net,
            })
            .collect()
    }
    #[getter]
    fn opaque_records(&self) -> Vec<PyOpaqueRegion> {
        self.inner()
            .opaque_records
            .iter()
            .cloned()
            .map(|inner| PyOpaqueRegion {
                owner: self.owner.project().clone(),
                inner,
            })
            .collect()
    }
}
#[pyclass(name = "StructuredNode", module = "gxwlib._core", frozen)]
pub(super) struct PyStructuredNode {
    pub(super) owner: Arc<s::StructuredProgram>,
    block: usize,
    node: usize,
}
impl PyStructuredNode {
    fn inner(&self) -> &s::StructuredNode {
        &self.owner.blocks[self.block].nodes[self.node]
    }
}
#[pymethods]
impl PyStructuredNode {
    #[getter]
    fn ordinal(&self) -> usize {
        self.inner().ordinal
    }
    #[getter]
    fn kind_code(&self) -> u32 {
        self.inner().kind_code
    }
    #[getter]
    fn kind(&self) -> String {
        self.inner().kind.clone()
    }
    #[getter]
    fn symbol(&self) -> String {
        self.inner().symbol.clone()
    }
    #[getter]
    fn type_name(&self) -> Option<String> {
        self.inner().type_name.clone()
    }
    #[getter]
    fn object_flag(&self) -> Option<u32> {
        self.inner().object_flag
    }
    #[getter]
    fn reserved(&self) -> Option<u16> {
        self.inner().reserved
    }
    #[getter]
    fn bounds(&self) -> Vec<u32> {
        self.inner().bounds.to_vec()
    }
    #[getter]
    fn source(&self) -> PySourceSpan {
        span(self.owner.project(), &self.inner().source)
    }
    #[getter]
    fn ports(&self) -> Vec<PyStructuredPort> {
        (0..self.inner().ports.len())
            .map(|port| PyStructuredPort {
                owner: self.owner.clone(),
                block: self.block,
                node: self.node,
                port,
            })
            .collect()
    }
}
#[pyclass(name = "StructuredPort", module = "gxwlib._core", frozen)]
pub(super) struct PyStructuredPort {
    pub(super) owner: Arc<s::StructuredProgram>,
    block: usize,
    node: usize,
    port: usize,
}
impl PyStructuredPort {
    fn inner(&self) -> &s::StructuredPort {
        &self.owner.blocks[self.block].nodes[self.node].ports[self.port]
    }
}
#[pymethods]
impl PyStructuredPort {
    #[getter]
    fn index(&self) -> usize {
        self.inner().index
    }
    #[getter]
    fn kind_code(&self) -> u32 {
        self.inner().kind_code
    }
    #[getter]
    fn local(&self) -> (u32, u32) {
        (self.inner().local.x, self.inner().local.y)
    }
    #[getter]
    fn position(&self) -> (u32, u32) {
        (self.inner().position.x, self.inner().position.y)
    }
    #[getter]
    fn source(&self) -> PySourceSpan {
        span(self.owner.project(), &self.inner().source)
    }
}
#[pyclass(name = "StructuredWire", module = "gxwlib._core", frozen)]
pub(super) struct PyStructuredWire {
    pub(super) owner: Arc<s::StructuredProgram>,
    block: usize,
    wire: usize,
}
impl PyStructuredWire {
    fn inner(&self) -> &s::StructuredWire {
        &self.owner.blocks[self.block].wires[self.wire]
    }
}
#[pymethods]
impl PyStructuredWire {
    #[getter]
    fn ordinal(&self) -> usize {
        self.inner().ordinal
    }
    #[getter]
    fn start(&self) -> (u32, u32) {
        (self.inner().start.x, self.inner().start.y)
    }
    #[getter]
    fn end(&self) -> (u32, u32) {
        (self.inner().end.x, self.inner().end.y)
    }
    #[getter]
    fn flags(&self) -> Vec<u32> {
        self.inner().flags.to_vec()
    }
    #[getter]
    fn suffix(&self) -> u32 {
        self.inner().suffix
    }
    #[getter]
    fn source(&self) -> PySourceSpan {
        span(self.owner.project(), &self.inner().source)
    }
}
#[pyclass(name = "StructuredNet", module = "gxwlib._core", frozen)]
pub(super) struct PyStructuredNet {
    pub(super) owner: Arc<s::StructuredProgram>,
    block: usize,
    net: usize,
}
impl PyStructuredNet {
    fn inner(&self) -> &s::StructuredNet {
        &self.owner.blocks[self.block].nets[self.net]
    }
}
#[pymethods]
impl PyStructuredNet {
    #[getter]
    fn index(&self) -> usize {
        self.inner().index
    }
    #[getter]
    fn wires(&self) -> Vec<usize> {
        self.inner().wires.clone()
    }
    #[getter]
    fn ports(&self) -> Vec<(usize, usize)> {
        self.inner()
            .ports
            .iter()
            .map(|p| (p.node, p.port))
            .collect()
    }
}
#[pyclass(name = "DeclarationTable", module = "gxwlib._core", frozen)]
pub(super) struct PyDeclarationTable {
    pub(super) owner: Arc<s::DeclarationTable>,
}
impl PyDeclarationTable {
    fn inner(&self) -> &s::DeclarationTable {
        &self.owner
    }
}
#[pymethods]
impl PyDeclarationTable {
    #[getter]
    fn schema_version(&self) -> u32 {
        self.inner().schema_version
    }
    #[getter]
    fn source_sha256(&self) -> String {
        self.inner().source_sha256.clone()
    }
    #[getter]
    fn logical_index(&self) -> usize {
        self.inner().logical_index
    }
    #[getter]
    fn logical_name(&self) -> String {
        self.inner().logical_name.clone()
    }
    #[getter]
    fn scope(&self) -> String {
        self.inner().scope.clone()
    }
    #[getter]
    fn owner_name(&self) -> Option<String> {
        self.inner().owner_name.clone()
    }
    #[getter]
    fn source(&self) -> PySourceSpan {
        span(self.owner.project(), &self.inner().source)
    }
    #[getter]
    fn header(&self) -> PySourceSpan {
        span(self.owner.project(), &self.inner().header)
    }
    #[getter]
    fn trailer(&self) -> PySourceSpan {
        span(self.owner.project(), &self.inner().trailer)
    }
    #[getter]
    fn structure_complete(&self) -> bool {
        self.inner().structure_complete
    }
    #[getter]
    fn rows(&self) -> Vec<PyLabelDeclaration> {
        (0..self.inner().rows.len())
            .map(|row| PyLabelDeclaration {
                owner: self.owner.clone(),
                row,
            })
            .collect()
    }
    #[getter]
    fn diagnostics(&self) -> Vec<PyParseDiagnostic> {
        self.inner()
            .diagnostics
            .iter()
            .cloned()
            .map(|inner| PyParseDiagnostic {
                owner: self.owner.project().clone(),
                inner,
            })
            .collect()
    }
    fn to_json(&self, py: Python<'_>) -> PyResult<String> {
        let owner = self.owner.clone();
        py.detach(move || owner.to_json())
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
}
#[pyclass(name = "LabelDeclaration", module = "gxwlib._core", frozen)]
pub(super) struct PyLabelDeclaration {
    pub(super) owner: Arc<s::DeclarationTable>,
    row: usize,
}
impl PyLabelDeclaration {
    fn inner(&self) -> &s::LabelDeclaration {
        &self.owner.rows[self.row]
    }
}
#[pymethods]
impl PyLabelDeclaration {
    #[getter]
    fn ordinal(&self) -> usize {
        self.inner().ordinal
    }
    #[getter]
    fn name(&self) -> String {
        self.inner().name.clone()
    }
    #[getter]
    fn data_type(&self) -> String {
        self.inner().data_type.clone()
    }
    #[getter]
    fn class_code(&self) -> u32 {
        self.inner().class_code
    }
    #[getter]
    fn device(&self) -> String {
        self.inner().device.clone()
    }
    #[getter]
    fn iec_address(&self) -> String {
        self.inner().iec_address.clone()
    }
    #[getter]
    fn unknown_u32(&self) -> u32 {
        self.inner().unknown_u32
    }
    #[getter]
    fn initial_value(&self) -> String {
        self.inner().initial_value.clone()
    }
    #[getter]
    fn unknown_text(&self) -> String {
        self.inner().unknown_text.clone()
    }
    #[getter]
    fn record_id(&self) -> u32 {
        self.inner().record_id
    }
    #[getter]
    fn comment(&self) -> String {
        self.inner().comment.clone()
    }
    #[getter]
    fn array_marker(&self) -> u32 {
        self.inner().array_marker
    }
    #[getter]
    fn type_code(&self) -> u32 {
        self.inner().type_code
    }
    #[getter]
    fn type_reference(&self) -> String {
        self.inner().type_reference.clone()
    }
    #[getter]
    fn source(&self) -> PySourceSpan {
        span(self.owner.project(), &self.inner().source)
    }
}
#[pyclass(name = "StructuredSvgDocument", module = "gxwlib._core", frozen)]
pub(super) struct PyStructuredSvgDocument {
    pub(super) owner: Arc<s::render::StructuredSvgDocument>,
}
impl PyStructuredSvgDocument {
    fn inner(&self) -> &s::render::StructuredSvgDocument {
        &self.owner
    }
}
#[pymethods]
impl PyStructuredSvgDocument {
    #[getter]
    fn schema_version(&self) -> u32 {
        self.inner().schema_version
    }
    #[getter]
    fn source_sha256(&self) -> String {
        self.inner().source_sha256.clone()
    }
    #[getter]
    fn layout(&self) -> String {
        self.inner().layout.clone()
    }
    #[getter]
    fn structure_complete(&self) -> bool {
        self.inner().structure_complete
    }
    #[getter]
    fn width(&self) -> u64 {
        self.inner().width
    }
    #[getter]
    fn height(&self) -> u64 {
        self.inner().height
    }
    #[getter]
    fn svg(&self) -> String {
        self.inner().svg.clone()
    }
    #[getter]
    fn elements(&self) -> Vec<PyStructuredSvgElement> {
        (0..self.inner().elements.len())
            .map(|element| PyStructuredSvgElement {
                owner: self.owner.clone(),
                element,
            })
            .collect()
    }
    fn to_json(&self, py: Python<'_>) -> PyResult<String> {
        let owner = self.owner.clone();
        py.detach(move || owner.to_json())
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
    fn to_html(&self, py: Python<'_>) -> PyResult<String> {
        let owner = self.owner.clone();
        py.detach(move || owner.to_html()).map_err(convert_error)
    }
    fn _repr_svg_(&self) -> &str {
        &self.owner.svg
    }
}
#[pyclass(name = "StructuredSvgElement", module = "gxwlib._core", frozen)]
pub(super) struct PyStructuredSvgElement {
    pub(super) owner: Arc<s::render::StructuredSvgDocument>,
    element: usize,
}
impl PyStructuredSvgElement {
    fn inner(&self) -> &s::render::StructuredSvgElement {
        &self.owner.elements[self.element]
    }
}
#[pymethods]
impl PyStructuredSvgElement {
    #[getter]
    fn id(&self) -> String {
        self.inner().id.clone()
    }
    #[getter]
    fn block(&self) -> usize {
        self.inner().block
    }
    #[getter]
    fn kind(&self) -> String {
        self.inner().kind.clone()
    }
    #[getter]
    fn label(&self) -> String {
        self.inner().label.clone()
    }
    #[getter]
    fn source(&self) -> PySourceSpan {
        span(self.owner.program.project(), &self.inner().source)
    }
}

#[pyfunction]
#[pyo3(signature=(project,program_index,*,limits=None))]
fn decode_structured(
    py: Python<'_>,
    project: PyRef<'_, PyParsedProject>,
    program_index: usize,
    limits: Option<PyRef<'_, PyStructuredLimits>>,
) -> PyResult<PyStructuredProgram> {
    let project = project.inner.clone();
    let limits = limits.map(|v| v.inner.clone()).unwrap_or_default();
    py.detach(move || s::decode_structured(project, program_index, &limits))
        .map(|p| PyStructuredProgram { owner: Arc::new(p) })
        .map_err(convert_error)
}
#[pyfunction]
#[pyo3(signature=(project,logical_index,*,limits=None))]
fn decode_declarations(
    py: Python<'_>,
    project: PyRef<'_, PyParsedProject>,
    logical_index: usize,
    limits: Option<PyRef<'_, PyStructuredLimits>>,
) -> PyResult<PyDeclarationTable> {
    let project = project.inner.clone();
    let limits = limits.map(|v| v.inner.clone()).unwrap_or_default();
    py.detach(move || s::decode_declarations(project, logical_index, &limits))
        .map(|p| PyDeclarationTable { owner: Arc::new(p) })
        .map_err(convert_error)
}
#[pyfunction]
#[pyo3(signature=(project,program_index,*,device_profile,limits=None))]
fn decode_ir(
    py: Python<'_>,
    project: PyRef<'_, PyParsedProject>,
    program_index: usize,
    device_profile: &str,
    limits: Option<PyRef<'_, PyStructuredLimits>>,
) -> PyResult<Py<PyAny>> {
    let project = project.inner.clone();
    let limits = limits.map(|v| v.inner.clone()).unwrap_or_default();
    let profile = profile(device_profile)?;
    match py
        .detach(move || s::decode_ir(project, program_index, profile, &limits))
        .map_err(convert_error)?
    {
        s::ProgramIr::Structured(p) => Ok(Py::new(
            py,
            PyStructuredProgram {
                owner: Arc::new(*p),
            },
        )?
        .into_any()),
        s::ProgramIr::Instructions(p) => Ok(Py::new(
            py,
            PyInstructionProgram {
                inner: Arc::new(*p),
            },
        )?
        .into_any()),
    }
}
#[pyfunction]
#[pyo3(signature=(program,*,max_elements=20_000,max_output_bytes=16_777_216))]
fn render_structured(
    py: Python<'_>,
    program: PyRef<'_, PyStructuredProgram>,
    max_elements: usize,
    max_output_bytes: usize,
) -> PyResult<PyStructuredSvgDocument> {
    let owner = program.owner.clone();
    py.detach(move || {
        s::render::render_structured(
            owner,
            &core::RenderOptions {
                max_elements,
                max_output_bytes,
            },
        )
    })
    .map(|p| PyStructuredSvgDocument { owner: Arc::new(p) })
    .map_err(convert_error)
}
#[pyclass(name = "DeviceAddress", module = "gxwlib._core", frozen)]
struct PyDeviceAddress {
    inner: s::devices::DeviceAddress,
}
#[pymethods]
impl PyDeviceAddress {
    #[getter]
    fn family(&self) -> &str {
        self.inner.family.as_str()
    }
    #[getter]
    fn device(&self) -> String {
        format!("{:?}", self.inner.device)
    }
    #[getter]
    fn address(&self) -> u32 {
        self.inner.address
    }
    #[getter]
    fn radix(&self) -> u32 {
        self.inner.radix
    }
    #[getter]
    fn canonical(&self) -> &str {
        &self.inner.canonical
    }
    #[getter]
    fn range_status(&self) -> &str {
        &self.inner.range_status
    }
}
#[pyfunction]
#[pyo3(signature=(text,*,family))]
fn parse_device_address(text: &str, family: &str) -> PyResult<PyDeviceAddress> {
    let family = match family {
        "fx" => s::devices::CpuFamily::Fx,
        "q" => s::devices::CpuFamily::Q,
        "l" => s::devices::CpuFamily::L,
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "family must be 'fx', 'q' or 'l'",
            ));
        }
    };
    s::devices::parse_device_address(text, family)
        .map(|inner| PyDeviceAddress { inner })
        .map_err(convert_error)
}
pub(super) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyDeviceAddress>()?;
    m.add_function(wrap_pyfunction!(parse_device_address, m)?)?;
    m.add_class::<PyStructuredLimits>()?;
    m.add_class::<PyStructuredProgram>()?;
    m.add_class::<PyStructuredBlock>()?;
    m.add_class::<PyStructuredNode>()?;
    m.add_class::<PyStructuredPort>()?;
    m.add_class::<PyStructuredWire>()?;
    m.add_class::<PyStructuredNet>()?;
    m.add_class::<PyDeclarationTable>()?;
    m.add_class::<PyLabelDeclaration>()?;
    m.add_class::<PyStructuredSvgDocument>()?;
    m.add_class::<PyStructuredSvgElement>()?;
    m.add_function(wrap_pyfunction!(decode_structured, m)?)?;
    m.add_function(wrap_pyfunction!(decode_declarations, m)?)?;
    m.add_function(wrap_pyfunction!(decode_ir, m)?)?;
    m.add_function(wrap_pyfunction!(render_structured, m)?)?;
    Ok(())
}
