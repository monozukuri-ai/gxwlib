use super::{PyProjectIndex, PyReadLimits, PyStreamLocation, convert_error};
use gxw_core as core;
use pyo3::{
    exceptions::{PyRuntimeError, PyValueError},
    prelude::*,
    types::PyBytes,
};
use std::{path::PathBuf, sync::Arc};

#[pyclass(name = "SourceSpan", module = "gxwlib._core", frozen)]
pub(super) struct PySourceSpan {
    owner: Arc<core::ParsedProject>,
    inner: core::SourceSpan,
}

#[pymethods]
impl PySourceSpan {
    #[getter]
    fn source_id(&self) -> u32 {
        self.inner.source_id
    }
    #[getter]
    fn offset(&self) -> u64 {
        self.inner.offset
    }
    #[getter]
    fn length(&self) -> u64 {
        self.inner.length
    }
}

pub(super) fn span(owner: &Arc<core::ParsedProject>, value: &core::SourceSpan) -> PySourceSpan {
    PySourceSpan {
        owner: owner.clone(),
        inner: value.clone(),
    }
}

#[pyclass(name = "SourceInfo", module = "gxwlib._core", frozen)]
struct PySourceInfo {
    owner: Arc<core::ParsedProject>,
    inner: core::SourceInfo,
}

#[pymethods]
impl PySourceInfo {
    #[getter]
    fn source(&self) -> PySourceSpan {
        span(
            &self.owner,
            &core::SourceSpan {
                source_id: self.inner.id,
                offset: 0,
                length: self.inner.size,
            },
        )
    }
    #[getter]
    fn id(&self) -> u32 {
        self.inner.id
    }
    #[getter]
    fn location(&self) -> PyStreamLocation {
        PyStreamLocation {
            inner: self.inner.location.clone(),
        }
    }
    #[getter]
    fn size(&self) -> u64 {
        self.inner.size
    }
    #[getter]
    fn sha256(&self) -> &str {
        &self.inner.sha256
    }
}

#[pyclass(name = "ParseDiagnostic", module = "gxwlib._core", frozen)]
pub(super) struct PyParseDiagnostic {
    pub(super) owner: Arc<core::ParsedProject>,
    pub(super) inner: core::ParseDiagnostic,
}

#[pymethods]
impl PyParseDiagnostic {
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
    fn source(&self) -> Option<PySourceSpan> {
        self.inner.source.as_ref().map(|s| span(&self.owner, s))
    }
}

#[pyclass(name = "OpaqueRegion", module = "gxwlib._core", frozen)]
pub(super) struct PyOpaqueRegion {
    pub(super) owner: Arc<core::ParsedProject>,
    pub(super) inner: core::OpaqueRegion,
}

#[pymethods]
impl PyOpaqueRegion {
    #[getter]
    fn source(&self) -> PySourceSpan {
        span(&self.owner, &self.inner.source)
    }
    #[getter]
    fn reason(&self) -> &str {
        &self.inner.reason
    }
}

#[pyclass(name = "RawToken", module = "gxwlib._core", frozen)]
struct PyRawToken {
    owner: Arc<core::ParsedProject>,
    program: usize,
    token: usize,
}

impl PyRawToken {
    fn inner(&self) -> &core::RawToken {
        &self.owner.programs[self.program].tokens[self.token]
    }
    fn bytes(&self) -> &[u8] {
        self.owner
            .source_bytes(&self.inner().source)
            .expect("validated token range")
    }
}

#[pymethods]
impl PyRawToken {
    #[getter]
    fn ordinal(&self) -> usize {
        self.inner().ordinal
    }
    #[getter]
    fn source(&self) -> PySourceSpan {
        span(&self.owner, &self.inner().source)
    }
    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, self.bytes())
    }
    #[getter]
    fn data_hex(&self) -> String {
        self.bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[pyclass(name = "RawProgram", module = "gxwlib._core", frozen)]
struct PyRawProgram {
    owner: Arc<core::ParsedProject>,
    index: usize,
}

impl PyRawProgram {
    fn inner(&self) -> &core::RawProgram {
        &self.owner.programs[self.index]
    }
}

#[pymethods]
impl PyRawProgram {
    #[getter]
    fn logical_index(&self) -> usize {
        self.inner().logical_index
    }
    #[getter]
    fn logical_name(&self) -> Option<&str> {
        self.inner().logical_name.as_deref()
    }
    #[getter]
    fn profile(&self) -> Option<&str> {
        self.inner().profile.as_deref()
    }
    #[getter]
    fn framing_status(&self) -> &'static str {
        self.inner().framing_status.as_str()
    }
    #[getter]
    fn metadata_source(&self) -> PySourceSpan {
        span(&self.owner, &self.inner().metadata_source)
    }
    #[getter]
    fn source(&self) -> Option<PySourceSpan> {
        self.inner().source.as_ref().map(|s| span(&self.owner, s))
    }
    #[getter]
    fn token_region(&self) -> Option<PySourceSpan> {
        self.inner()
            .token_region
            .as_ref()
            .map(|s| span(&self.owner, s))
    }
    #[getter]
    fn trailer(&self) -> Option<PySourceSpan> {
        self.inner().trailer.as_ref().map(|s| span(&self.owner, s))
    }
    #[getter]
    fn tokens(&self) -> Vec<PyRawToken> {
        (0..self.inner().tokens.len())
            .map(|token| PyRawToken {
                owner: self.owner.clone(),
                program: self.index,
                token,
            })
            .collect()
    }
    #[getter]
    fn opaque_regions(&self) -> Vec<PyOpaqueRegion> {
        self.inner()
            .opaque_regions
            .iter()
            .cloned()
            .map(|inner| PyOpaqueRegion {
                owner: self.owner.clone(),
                inner,
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
                owner: self.owner.clone(),
                inner,
            })
            .collect()
    }
}

#[pyclass(name = "ParsedProject", module = "gxwlib._core", frozen)]
pub(super) struct PyParsedProject {
    pub(super) inner: Arc<core::ParsedProject>,
}

#[pymethods]
impl PyParsedProject {
    #[getter]
    fn schema_version(&self) -> u32 {
        1
    }
    #[getter]
    fn index(&self) -> PyProjectIndex {
        PyProjectIndex {
            inner: self.inner.index.clone(),
        }
    }
    #[getter]
    fn semantic_status(&self) -> &'static str {
        "not_decoded"
    }
    #[getter]
    fn cpu_model(&self) -> Option<String> {
        None
    }
    #[getter]
    fn execution_order(&self) -> Option<Vec<usize>> {
        None
    }
    #[getter]
    fn framing_complete(&self) -> bool {
        self.inner.framing_complete()
    }
    #[getter]
    fn sources(&self) -> Vec<PySourceInfo> {
        self.inner
            .sources()
            .iter()
            .cloned()
            .map(|inner| PySourceInfo {
                owner: self.inner.clone(),
                inner,
            })
            .collect()
    }
    #[getter]
    fn programs(&self) -> Vec<PyRawProgram> {
        (0..self.inner.programs.len())
            .map(|index| PyRawProgram {
                owner: self.inner.clone(),
                index,
            })
            .collect()
    }
    #[getter]
    fn diagnostics(&self) -> Vec<PyParseDiagnostic> {
        self.inner
            .diagnostics
            .iter()
            .cloned()
            .map(|inner| PyParseDiagnostic {
                owner: self.inner.clone(),
                inner,
            })
            .collect()
    }
    fn read_source<'py>(
        &self,
        py: Python<'py>,
        source: PyRef<'_, PySourceSpan>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        if !Arc::ptr_eq(&self.inner, &source.owner) {
            return Err(PyValueError::new_err(
                "source span belongs to a different project",
            ));
        }
        let data = self
            .inner
            .source_bytes(&source.inner)
            .ok_or_else(|| PyValueError::new_err("invalid source range"))?;
        Ok(PyBytes::new(py, data))
    }
    fn to_json(&self, py: Python<'_>) -> PyResult<String> {
        let owner = self.inner.clone();
        py.detach(move || owner.to_json())
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
}

#[pyfunction]
#[pyo3(signature = (path, *, limits=None))]
fn load_project(
    py: Python<'_>,
    path: PathBuf,
    limits: Option<PyRef<'_, PyReadLimits>>,
) -> PyResult<PyParsedProject> {
    let limits = limits.map(|l| l.inner.clone()).unwrap_or_default();
    let project = py
        .detach(move || core::load_path(&path, &limits))
        .map_err(convert_error)?;
    Ok(PyParsedProject {
        inner: Arc::new(project),
    })
}

#[pyfunction]
#[pyo3(signature = (data, *, limits=None))]
fn loads(
    py: Python<'_>,
    data: &Bound<'_, PyBytes>,
    limits: Option<PyRef<'_, PyReadLimits>>,
) -> PyResult<PyParsedProject> {
    let limits = limits.map(|l| l.inner.clone()).unwrap_or_default();
    if data.as_bytes().len() as u64 > limits.max_file_bytes {
        return Err(convert_error(core::GxwError::ResourceLimit {
            resource: "input bytes".into(),
            actual: data.as_bytes().len() as u64,
            limit: limits.max_file_bytes,
        }));
    }
    let bytes = Arc::from(data.as_bytes());
    let project = py
        .detach(move || core::parse_bytes(bytes, &limits))
        .map_err(convert_error)?;
    Ok(PyParsedProject {
        inner: Arc::new(project),
    })
}

pub(super) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySourceSpan>()?;
    m.add_class::<PySourceInfo>()?;
    m.add_class::<PyParseDiagnostic>()?;
    m.add_class::<PyOpaqueRegion>()?;
    m.add_class::<PyRawToken>()?;
    m.add_class::<PyRawProgram>()?;
    m.add_class::<PyParsedProject>()?;
    m.add_function(wrap_pyfunction!(load_project, m)?)?;
    m.add_function(wrap_pyfunction!(loads, m)?)?;
    Ok(())
}
