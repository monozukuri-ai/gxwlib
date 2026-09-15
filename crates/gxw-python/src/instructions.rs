use super::{PyReadLimits, convert_error, raw::PyParsedProject};
use gxw_core as core;
use pyo3::{
    exceptions::{PyRuntimeError, PyValueError},
    prelude::*,
    types::PyBytes,
};
use std::{path::PathBuf, sync::Arc};
pub(super) fn profile(value: &str) -> PyResult<core::DeviceProfile> {
    if value == "fx" {
        Ok(core::DeviceProfile::Fx)
    } else {
        Err(PyValueError::new_err("device_profile must be 'fx'"))
    }
}
#[pyclass(name = "InstructionSpan", module = "gxwlib._core", frozen)]
pub(super) struct PyInstructionSpan {
    pub(super) owner: Arc<core::InstructionProgram>,
    pub(super) inner: core::InstructionSpan,
}
#[pymethods]
impl PyInstructionSpan {
    #[getter]
    fn source_id(&self) -> Option<u32> {
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
#[pyclass(name = "InstructionDiagnostic", module = "gxwlib._core", frozen)]
struct PyInstructionDiagnostic {
    owner: Arc<core::InstructionProgram>,
    inner: core::InstructionDiagnostic,
}
#[pymethods]
impl PyInstructionDiagnostic {
    #[getter]
    fn code(&self) -> &str {
        &self.inner.code
    }
    #[getter]
    fn message(&self) -> &str {
        &self.inner.message
    }
    #[getter]
    fn source(&self) -> Option<PyInstructionSpan> {
        self.inner.source.as_ref().map(|inner| PyInstructionSpan {
            owner: self.owner.clone(),
            inner: inner.clone(),
        })
    }
}
#[pyclass(name = "Operand", module = "gxwlib._core", frozen)]
struct PyOperand {
    owner: Arc<core::InstructionProgram>,
    inner: core::Operand,
}
#[pymethods]
impl PyOperand {
    #[getter]
    fn original_text(&self) -> Option<&str> {
        self.inner.original_text.as_deref()
    }
    #[getter]
    fn encoding_width_bits(&self) -> Option<u8> {
        self.inner.encoding_width_bits
    }
    #[getter]
    fn value(&self) -> PyOperandValue {
        PyOperandValue {
            inner: self.inner.value.clone(),
        }
    }
    #[getter]
    fn source(&self) -> PyInstructionSpan {
        PyInstructionSpan {
            owner: self.owner.clone(),
            inner: self.inner.source.clone(),
        }
    }
}
#[pyclass(name = "CsvRow", module = "gxwlib._core", frozen)]
struct PyCsvRow {
    owner: Arc<core::InstructionProgram>,
    inner: core::CsvRow,
}
#[pymethods]
impl PyCsvRow {
    #[getter]
    fn number(&self) -> usize {
        self.inner.number
    }
    #[getter]
    fn fields(&self) -> Vec<String> {
        self.inner.fields.clone()
    }
    #[getter]
    fn source(&self) -> PyInstructionSpan {
        PyInstructionSpan {
            owner: self.owner.clone(),
            inner: self.inner.source.clone(),
        }
    }
    #[getter]
    fn field_sources(&self) -> Vec<PyInstructionSpan> {
        self.inner
            .field_sources
            .iter()
            .map(|inner| PyInstructionSpan {
                owner: self.owner.clone(),
                inner: inner.clone(),
            })
            .collect()
    }
}
#[pyclass(name = "OperandValue", module = "gxwlib._core", frozen)]
struct PyOperandValue {
    inner: core::OperandValue,
}
#[pymethods]
impl PyOperandValue {
    #[getter]
    fn kind(&self) -> &str {
        match self.inner {
            core::OperandValue::Device { .. } => "device",
            core::OperandValue::Constant { .. } => "constant",
            core::OperandValue::Unknown => "unknown",
        }
    }
    #[getter]
    fn device(&self) -> Option<&str> {
        match self.inner {
            core::OperandValue::Device { device, .. } => Some(match device {
                core::DeviceKind::X => "X",
                core::DeviceKind::Y => "Y",
                core::DeviceKind::M => "M",
                core::DeviceKind::D => "D",
                core::DeviceKind::T => "T",
                core::DeviceKind::C => "C",
            }),
            _ => None,
        }
    }
    #[getter]
    fn address(&self) -> Option<u32> {
        match self.inner {
            core::OperandValue::Device { address, .. } => Some(address),
            _ => None,
        }
    }
    #[getter]
    fn value(&self) -> Option<i64> {
        match self.inner {
            core::OperandValue::Constant { value, .. } => Some(value),
            _ => None,
        }
    }
    #[getter]
    fn radix(&self) -> Option<u8> {
        match self.inner {
            core::OperandValue::Device { radix, .. }
            | core::OperandValue::Constant { radix, .. } => Some(radix),
            _ => None,
        }
    }
    #[getter]
    fn bit_width(&self) -> Option<u8> {
        match self.inner {
            core::OperandValue::Constant { bit_width, .. } => bit_width,
            _ => None,
        }
    }
}
#[pyclass(name = "Instruction", module = "gxwlib._core", frozen)]
struct PyInstruction {
    owner: Arc<core::InstructionProgram>,
    index: usize,
}
impl PyInstruction {
    fn inner(&self) -> &core::Instruction {
        &self.owner.instructions[self.index]
    }
}
#[pymethods]
impl PyInstruction {
    #[getter]
    fn ordinal(&self) -> usize {
        self.inner().ordinal
    }
    #[getter]
    fn step(&self) -> Option<u32> {
        self.inner().step
    }
    #[getter]
    fn opcode(&self) -> Option<&str> {
        self.inner().opcode.map(|o| o.as_str())
    }
    #[getter]
    fn mnemonic(&self) -> Option<&str> {
        self.inner().mnemonic.as_deref()
    }
    #[getter]
    fn original_mnemonic(&self) -> Option<&str> {
        self.inner().original_mnemonic.as_deref()
    }
    #[getter]
    fn supported(&self) -> bool {
        self.inner().supported
    }
    #[getter]
    fn source(&self) -> PyInstructionSpan {
        PyInstructionSpan {
            owner: self.owner.clone(),
            inner: self.inner().source.clone(),
        }
    }
    #[getter]
    fn operands(&self) -> Vec<PyOperand> {
        self.inner()
            .operands
            .iter()
            .map(|inner| PyOperand {
                owner: self.owner.clone(),
                inner: inner.clone(),
            })
            .collect()
    }
}
#[pyclass(name = "InstructionProgram", module = "gxwlib._core", frozen)]
pub(super) struct PyInstructionProgram {
    pub(super) inner: Arc<core::InstructionProgram>,
}
#[pymethods]
impl PyInstructionProgram {
    #[getter]
    fn schema_version(&self) -> u32 {
        self.inner.schema_version
    }
    #[getter]
    fn origin(&self) -> &str {
        &self.inner.origin
    }
    #[getter]
    fn source_sha256(&self) -> &str {
        &self.inner.source_sha256
    }
    #[getter]
    fn device_profile(&self) -> &str {
        "fx"
    }
    #[getter]
    fn logical_name(&self) -> Option<&str> {
        self.inner.logical_name.as_deref()
    }
    #[getter]
    fn complete(&self) -> bool {
        self.inner.complete
    }
    #[getter]
    fn instructions(&self) -> Vec<PyInstruction> {
        (0..self.inner.instructions.len())
            .map(|index| PyInstruction {
                owner: self.inner.clone(),
                index,
            })
            .collect()
    }
    #[getter]
    fn diagnostics(&self) -> Vec<PyInstructionDiagnostic> {
        self.inner
            .diagnostics
            .iter()
            .map(|inner| PyInstructionDiagnostic {
                owner: self.inner.clone(),
                inner: inner.clone(),
            })
            .collect()
    }
    #[getter]
    fn opaque_regions(&self) -> Vec<PyInstructionSpan> {
        self.inner
            .opaque_regions
            .iter()
            .map(|inner| PyInstructionSpan {
                owner: self.inner.clone(),
                inner: inner.clone(),
            })
            .collect()
    }
    #[getter]
    fn csv_rows(&self) -> Vec<PyCsvRow> {
        self.inner
            .csv_rows
            .iter()
            .map(|inner| PyCsvRow {
                owner: self.inner.clone(),
                inner: inner.clone(),
            })
            .collect()
    }
    fn to_json(&self, py: Python<'_>) -> PyResult<String> {
        let owner = self.inner.clone();
        py.detach(move || owner.to_json())
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
    fn read_source<'py>(
        &self,
        py: Python<'py>,
        span: PyRef<'_, PyInstructionSpan>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        if !Arc::ptr_eq(&self.inner, &span.owner) {
            return Err(PyValueError::new_err(
                "source belongs to another InstructionProgram",
            ));
        }
        let bytes = self
            .inner
            .source_bytes(&span.inner)
            .ok_or_else(|| PyValueError::new_err("invalid source range"))?;
        Ok(PyBytes::new(py, bytes))
    }
}
#[pyfunction]
#[pyo3(signature = (project, program_index, *, device_profile))]
fn decode_program(
    py: Python<'_>,
    project: PyRef<'_, PyParsedProject>,
    program_index: usize,
    device_profile: &str,
) -> PyResult<PyInstructionProgram> {
    let owner = project.inner.clone();
    let profile = profile(device_profile)?;
    let inner = py
        .detach(move || core::decode_program(owner, program_index, profile))
        .map_err(convert_error)?;
    Ok(PyInstructionProgram {
        inner: Arc::new(inner),
    })
}
#[pyfunction]
#[pyo3(signature = (path, *, device_profile, limits=None))]
fn load_csv(
    py: Python<'_>,
    path: PathBuf,
    device_profile: &str,
    limits: Option<PyRef<'_, PyReadLimits>>,
) -> PyResult<PyInstructionProgram> {
    let profile = profile(device_profile)?;
    let limits = limits.map(|l| l.inner.clone()).unwrap_or_default();
    let inner = py
        .detach(move || core::load_csv(&path, profile, &limits))
        .map_err(convert_error)?;
    Ok(PyInstructionProgram {
        inner: Arc::new(inner),
    })
}
#[pyfunction]
#[pyo3(signature = (data, *, device_profile, limits=None))]
fn parse_csv_bytes(
    py: Python<'_>,
    data: &Bound<'_, PyBytes>,
    device_profile: &str,
    limits: Option<PyRef<'_, PyReadLimits>>,
) -> PyResult<PyInstructionProgram> {
    let profile = profile(device_profile)?;
    let limits = limits.map(|l| l.inner.clone()).unwrap_or_default();
    if data.as_bytes().len() as u64 > limits.max_file_bytes {
        return Err(convert_error(core::GxwError::ResourceLimit {
            resource: "input bytes".into(),
            actual: data.as_bytes().len() as u64,
            limit: limits.max_file_bytes,
        }));
    }
    let bytes: Arc<[u8]> = data.as_bytes().into();
    let inner = py
        .detach(move || core::parse_csv_bytes(bytes, profile, &limits))
        .map_err(convert_error)?;
    Ok(PyInstructionProgram {
        inner: Arc::new(inner),
    })
}
pub(super) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyInstructionSpan>()?;
    m.add_class::<PyInstructionDiagnostic>()?;
    m.add_class::<PyOperand>()?;
    m.add_class::<PyCsvRow>()?;
    m.add_class::<PyOperandValue>()?;
    m.add_class::<PyInstruction>()?;
    m.add_class::<PyInstructionProgram>()?;
    m.add_function(wrap_pyfunction!(decode_program, m)?)?;
    m.add_function(wrap_pyfunction!(load_csv, m)?)?;
    m.add_function(wrap_pyfunction!(parse_csv_bytes, m)?)?;
    Ok(())
}
