use gxw_core as core;
use pyo3::prelude::*;
use pyo3::{
    create_exception,
    exceptions::{PyException, PyRuntimeError},
    types::PyBytes,
};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
mod analysis;
mod instructions;
mod raw;
mod simulation;
mod structured;

create_exception!(gxwlib, GxwError, PyException);
create_exception!(gxwlib, FormatError, GxwError);
create_exception!(gxwlib, UnsupportedFeatureError, GxwError);
create_exception!(gxwlib, ResourceLimitError, GxwError);
create_exception!(gxwlib, SimulationError, GxwError);

fn convert_error(error: core::GxwError) -> PyErr {
    match error {
        core::GxwError::Io(e) => e.into(),
        e @ core::GxwError::Simulation { .. } => Python::attach(|py| {
            let err = SimulationError::new_err(e.to_string());
            if let core::GxwError::Simulation {
                code, instruction, ..
            } = e
                && let Err(e) = err
                    .value(py)
                    .setattr("code", code)
                    .and_then(|()| err.value(py).setattr("instruction", instruction))
            {
                return e;
            }
            err
        }),
        e @ core::GxwError::Format { .. } => FormatError::new_err(e.to_string()),
        e @ core::GxwError::Unsupported { .. } => UnsupportedFeatureError::new_err(e.to_string()),
        e @ core::GxwError::ResourceLimit { .. } => ResourceLimitError::new_err(e.to_string()),
    }
}

#[pyclass(name = "ReadLimits", module = "gxwlib._core", frozen)]
struct PyReadLimits {
    inner: core::ReadLimits,
}

#[pymethods]
impl PyReadLimits {
    #[new]
    #[pyo3(signature = (*, max_file_bytes=67_108_864, max_stream_bytes=67_108_864,
        max_total_stream_bytes=268_435_456, max_entries=65_536, max_xml_bytes=8_388_608,
        max_xml_depth=64, max_xml_rows=65_536, max_tokens=1_000_000))]
    #[allow(clippy::too_many_arguments)] // Mirrors the keyword-only resource bounds.
    fn new(
        max_file_bytes: u64,
        max_stream_bytes: u64,
        max_total_stream_bytes: u64,
        max_entries: u64,
        max_xml_bytes: u64,
        max_xml_depth: u64,
        max_xml_rows: u64,
        max_tokens: u64,
    ) -> Self {
        Self {
            inner: core::ReadLimits {
                max_file_bytes,
                max_stream_bytes,
                max_total_stream_bytes,
                max_entries,
                max_xml_bytes,
                max_xml_depth,
                max_xml_rows,
                max_tokens,
            },
        }
    }
    #[getter]
    fn max_file_bytes(&self) -> u64 {
        self.inner.max_file_bytes
    }
    #[getter]
    fn max_stream_bytes(&self) -> u64 {
        self.inner.max_stream_bytes
    }
    #[getter]
    fn max_total_stream_bytes(&self) -> u64 {
        self.inner.max_total_stream_bytes
    }
    #[getter]
    fn max_entries(&self) -> u64 {
        self.inner.max_entries
    }
    #[getter]
    fn max_xml_bytes(&self) -> u64 {
        self.inner.max_xml_bytes
    }
    #[getter]
    fn max_xml_depth(&self) -> u64 {
        self.inner.max_xml_depth
    }
    #[getter]
    fn max_xml_rows(&self) -> u64 {
        self.inner.max_xml_rows
    }
    #[getter]
    fn max_tokens(&self) -> u64 {
        self.inner.max_tokens
    }
}

#[pyclass(name = "ProjectIndex", module = "gxwlib._core", frozen)]
struct PyProjectIndex {
    inner: Arc<core::ProjectIndex>,
}

#[pymethods]
impl PyProjectIndex {
    #[getter]
    fn schema_version(&self) -> u32 {
        self.inner.schema_version
    }
    #[getter]
    fn size(&self) -> u64 {
        self.inner.size
    }
    #[getter]
    fn sha256(&self) -> &str {
        &self.inner.sha256
    }
    #[getter]
    fn containers(&self) -> Vec<PyContainerIndex> {
        (0..self.inner.containers.len())
            .map(|index| PyContainerIndex {
                owner: self.inner.clone(),
                index,
            })
            .collect()
    }
    #[getter]
    fn logical_objects(&self) -> Vec<PyLogicalObject> {
        (0..self.inner.logical_objects.len())
            .map(|index| PyLogicalObject {
                owner: self.inner.clone(),
                index,
            })
            .collect()
    }
    #[getter]
    fn project_rows(&self) -> Vec<PyMetadataRow> {
        self.inner
            .project_rows
            .iter()
            .cloned()
            .map(|inner| PyMetadataRow { inner })
            .collect()
    }
    #[getter]
    fn diagnostics(&self) -> Vec<PyDiagnostic> {
        self.inner
            .diagnostics
            .iter()
            .cloned()
            .map(|inner| PyDiagnostic { inner })
            .collect()
    }
    fn to_json(&self, py: Python<'_>) -> PyResult<String> {
        let owner = self.inner.clone();
        py.detach(move || owner.to_json())
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
    fn __repr__(&self) -> String {
        format!(
            "ProjectIndex(size={}, containers={}, logical_objects={})",
            self.inner.size,
            self.inner.containers.len(),
            self.inner.logical_objects.len()
        )
    }
}

#[pyclass(name = "ContainerIndex", module = "gxwlib._core", frozen)]
struct PyContainerIndex {
    owner: Arc<core::ProjectIndex>,
    index: usize,
}

#[pymethods]
impl PyContainerIndex {
    #[getter]
    fn path(&self) -> Vec<String> {
        self.owner.containers[self.index].path.clone()
    }
    #[getter]
    fn cfb_version(&self) -> u8 {
        self.owner.containers[self.index].cfb_version
    }
    #[getter]
    fn streams(&self) -> Vec<PyStreamInfo> {
        (0..self.owner.containers[self.index].streams.len())
            .map(|stream| PyStreamInfo {
                owner: self.owner.clone(),
                container: self.index,
                stream,
            })
            .collect()
    }
}

#[pyclass(name = "StreamInfo", module = "gxwlib._core", frozen)]
struct PyStreamInfo {
    owner: Arc<core::ProjectIndex>,
    container: usize,
    stream: usize,
}

impl PyStreamInfo {
    fn inner(&self) -> &core::StreamInfo {
        &self.owner.containers[self.container].streams[self.stream]
    }
}

#[pymethods]
impl PyStreamInfo {
    #[getter]
    fn path(&self) -> Vec<String> {
        self.inner().path.clone()
    }
    #[getter]
    fn size(&self) -> u64 {
        self.inner().size
    }
    #[getter]
    fn sha256(&self) -> &str {
        &self.inner().sha256
    }
}

#[pyclass(name = "StreamLocation", module = "gxwlib._core", frozen)]
struct PyStreamLocation {
    inner: core::StreamLocation,
}

#[pymethods]
impl PyStreamLocation {
    #[getter]
    fn container(&self) -> Vec<String> {
        self.inner.container.clone()
    }
    #[getter]
    fn path(&self) -> Vec<String> {
        self.inner.path.clone()
    }
}

#[pyclass(name = "MetadataRow", module = "gxwlib._core", frozen)]
struct PyMetadataRow {
    inner: core::MetadataRow,
}

#[pymethods]
impl PyMetadataRow {
    #[getter]
    fn fields(&self) -> BTreeMap<String, String> {
        self.inner.fields.clone()
    }
    #[getter]
    fn attributes(&self) -> BTreeMap<String, String> {
        self.inner.attributes.clone()
    }
    #[getter]
    fn ancestors(&self) -> Vec<String> {
        self.inner.ancestors.clone()
    }
}

#[pyclass(name = "LogicalObject", module = "gxwlib._core", frozen)]
struct PyLogicalObject {
    owner: Arc<core::ProjectIndex>,
    index: usize,
}

impl PyLogicalObject {
    fn inner(&self) -> &core::LogicalObject {
        &self.owner.logical_objects[self.index]
    }
}

#[pymethods]
impl PyLogicalObject {
    #[getter]
    fn id(&self) -> Option<&str> {
        self.inner().id.as_deref()
    }
    #[getter]
    fn logical_name(&self) -> Option<&str> {
        self.inner().logical_name.as_deref()
    }
    #[getter]
    fn scrap(&self) -> Option<bool> {
        self.inner().scrap
    }
    #[getter]
    fn historical(&self) -> bool {
        self.inner().historical
    }
    #[getter]
    fn metadata(&self) -> PyMetadataRow {
        PyMetadataRow {
            inner: self.inner().metadata.clone(),
        }
    }
    #[getter]
    fn location(&self) -> Option<PyStreamLocation> {
        self.inner()
            .location
            .clone()
            .map(|inner| PyStreamLocation { inner })
    }
}

#[pyclass(name = "Diagnostic", module = "gxwlib._core", frozen)]
struct PyDiagnostic {
    inner: core::Diagnostic,
}

#[pymethods]
impl PyDiagnostic {
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
    fn location(&self) -> Option<PyStreamLocation> {
        self.inner
            .location
            .clone()
            .map(|inner| PyStreamLocation { inner })
    }
}

#[pyfunction]
#[pyo3(signature = (path, *, limits=None))]
fn inspect(
    py: Python<'_>,
    path: PathBuf,
    limits: Option<PyRef<'_, PyReadLimits>>,
) -> PyResult<PyProjectIndex> {
    let limits = limits.map(|l| l.inner.clone()).unwrap_or_default();
    let index = py
        .detach(move || core::inspect_path(&path, &limits))
        .map_err(convert_error)?;
    Ok(PyProjectIndex {
        inner: Arc::new(index),
    })
}

#[pyfunction]
#[pyo3(signature = (data, *, limits=None))]
fn inspect_bytes(
    py: Python<'_>,
    data: &Bound<'_, PyBytes>,
    limits: Option<PyRef<'_, PyReadLimits>>,
) -> PyResult<PyProjectIndex> {
    let limits = limits.map(|l| l.inner.clone()).unwrap_or_default();
    if data.as_bytes().len() as u64 > limits.max_file_bytes {
        return Err(convert_error(core::GxwError::ResourceLimit {
            resource: "input bytes".into(),
            actual: data.as_bytes().len() as u64,
            limit: limits.max_file_bytes,
        }));
    }
    let bytes = data.as_bytes().to_vec();
    let index = py
        .detach(move || core::inspect_bytes(&bytes, &limits))
        .map_err(convert_error)?;
    Ok(PyProjectIndex {
        inner: Arc::new(index),
    })
}

#[pymodule(gil_used = true)]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_class::<PyReadLimits>()?;
    m.add_class::<PyProjectIndex>()?;
    m.add_class::<PyContainerIndex>()?;
    m.add_class::<PyStreamInfo>()?;
    m.add_class::<PyStreamLocation>()?;
    m.add_class::<PyMetadataRow>()?;
    m.add_class::<PyLogicalObject>()?;
    m.add_class::<PyDiagnostic>()?;
    m.add("GxwError", m.py().get_type::<GxwError>())?;
    m.add("FormatError", m.py().get_type::<FormatError>())?;
    m.add(
        "UnsupportedFeatureError",
        m.py().get_type::<UnsupportedFeatureError>(),
    )?;
    m.add(
        "ResourceLimitError",
        m.py().get_type::<ResourceLimitError>(),
    )?;
    m.add_function(wrap_pyfunction!(inspect, m)?)?;
    m.add_function(wrap_pyfunction!(inspect_bytes, m)?)?;
    raw::register(m)?;
    instructions::register(m)?;
    analysis::register(m)?;
    simulation::register(m)?;
    structured::register(m)?;
    m.add("SimulationError", m.py().get_type::<SimulationError>())?;
    Ok(())
}
