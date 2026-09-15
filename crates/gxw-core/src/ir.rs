//! Instruction syntax, not compiled PLC behavior or inferred CPU configuration.
use crate::{ParsedProject, SourceSpan};
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceProfile {
    Fx,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Opcode {
    Ld,
    Ldi,
    And,
    Ani,
    Or,
    Ori,
    Out,
    Set,
    Rst,
    Anb,
    Orb,
    Mps,
    Mrd,
    Mpp,
    End,
}

impl Opcode {
    pub fn from_mnemonic(text: &str) -> Option<Self> {
        Some(match text.to_ascii_uppercase().as_str() {
            "LD" => Self::Ld,
            "LDI" => Self::Ldi,
            "AND" => Self::And,
            "ANI" => Self::Ani,
            "OR" => Self::Or,
            "ORI" => Self::Ori,
            "OUT" => Self::Out,
            "SET" => Self::Set,
            "RST" => Self::Rst,
            "ANB" => Self::Anb,
            "ORB" => Self::Orb,
            "MPS" => Self::Mps,
            "MRD" => Self::Mrd,
            "MPP" => Self::Mpp,
            "END" => Self::End,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ld => "LD",
            Self::Ldi => "LDI",
            Self::And => "AND",
            Self::Ani => "ANI",
            Self::Or => "OR",
            Self::Ori => "ORI",
            Self::Out => "OUT",
            Self::Set => "SET",
            Self::Rst => "RST",
            Self::Anb => "ANB",
            Self::Orb => "ORB",
            Self::Mps => "MPS",
            Self::Mrd => "MRD",
            Self::Mpp => "MPP",
            Self::End => "END",
        }
    }
}

/// Original byte range: logical stream for GXW, entire file for CSV.
/// Every range is scoped to its InstructionProgram and source_sha256.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InstructionSpan {
    pub source_id: Option<u32>,
    pub offset: u64,
    pub length: u64,
}
impl From<&SourceSpan> for InstructionSpan {
    fn from(s: &SourceSpan) -> Self {
        Self {
            source_id: Some(s.source_id),
            offset: s.offset,
            length: s.length,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum DeviceKind {
    X,
    Y,
    M,
    D,
    T,
    C,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperandValue {
    Device {
        device: DeviceKind,
        address: u32,
        radix: u8,
    },
    Constant {
        value: i64,
        radix: u8,
        bit_width: Option<u8>,
    },
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct Operand {
    pub value: OperandValue,
    pub original_text: Option<String>,
    /// Number of payload bits in the compact encoding, not PLC register width.
    pub encoding_width_bits: Option<u8>,
    pub source: InstructionSpan,
}

#[derive(Debug, Clone, Serialize)]
pub struct Instruction {
    pub ordinal: usize,
    /// CSV step numbers are preserved, never inferred from byte offsets.
    pub step: Option<u32>,
    pub opcode: Option<Opcode>,
    /// Includes independently identified, but unsupported, instruction names.
    pub mnemonic: Option<String>,
    pub original_mnemonic: Option<String>,
    pub operands: Vec<Operand>,
    pub source: InstructionSpan,
    pub supported: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstructionDiagnostic {
    pub code: String,
    pub message: String,
    pub source: Option<InstructionSpan>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CsvRow {
    pub number: usize,
    pub fields: Vec<String>,
    pub source: InstructionSpan,
    pub field_sources: Vec<InstructionSpan>,
}

#[derive(Debug)]
pub(crate) enum InstructionBacking {
    Gxw(Arc<ParsedProject>),
    Csv(Arc<[u8]>),
}

#[derive(Debug, Serialize)]
pub struct InstructionProgram {
    pub schema_version: u32,
    pub origin: String,
    pub source_sha256: String,
    pub device_profile: DeviceProfile,
    pub logical_name: Option<String>,
    pub instructions: Vec<Instruction>,
    pub diagnostics: Vec<InstructionDiagnostic>,
    pub opaque_regions: Vec<InstructionSpan>,
    /// Includes the three metadata rows and all comments/continuations.
    pub csv_rows: Vec<CsvRow>,
    pub complete: bool,
    #[serde(skip)]
    pub(crate) backing: InstructionBacking,
}

impl InstructionProgram {
    pub fn source_bytes(&self, span: &InstructionSpan) -> Option<&[u8]> {
        match &self.backing {
            InstructionBacking::Gxw(p) => p.source_bytes(&SourceSpan {
                source_id: span.source_id?,
                offset: span.offset,
                length: span.length,
            }),
            InstructionBacking::Csv(data) => {
                if span.source_id.is_some() {
                    return None;
                }
                let start = usize::try_from(span.offset).ok()?;
                let end = usize::try_from(span.offset.checked_add(span.length)?).ok()?;
                data.get(start..end)
            }
        }
    }
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
    pub(crate) fn diagnostic(
        &mut self,
        code: &str,
        message: &str,
        source: Option<InstructionSpan>,
    ) {
        self.diagnostics.push(InstructionDiagnostic {
            code: code.into(),
            message: message.into(),
            source,
        });
    }
    pub(crate) fn finish(&mut self, framing_complete: bool) {
        for i in 0..self.instructions.len() {
            let ins = &mut self.instructions[i];
            ins.supported = valid_instruction(ins);
            if !ins.supported {
                self.diagnostic(
                    "GXW_INSTRUCTION_UNSUPPORTED",
                    "Instruction or operand form is outside the supported syntax subset",
                    Some(self.instructions[i].source.clone()),
                );
            }
        }
        let terminal = self
            .instructions
            .last()
            .is_some_and(|i| i.opcode == Some(Opcode::End));
        let early_end = self
            .instructions
            .iter()
            .take(self.instructions.len().saturating_sub(1))
            .any(|i| i.opcode == Some(Opcode::End));
        if !terminal || early_end {
            self.diagnostic(
                "GXW_INSTRUCTION_END_INVALID",
                "Expected one final END instruction",
                None,
            );
        }
        self.complete = framing_complete
            && self.opaque_regions.is_empty()
            && self.diagnostics.is_empty()
            && terminal
            && !early_end;
    }
}

pub(crate) fn valid_instruction(ins: &Instruction) -> bool {
    use DeviceKind::*;
    use OperandValue::*;
    let values: Vec<_> = ins.operands.iter().map(|o| &o.value).collect();
    match (ins.opcode, values.as_slice()) {
        (
            Some(Opcode::Anb | Opcode::Orb | Opcode::Mps | Opcode::Mrd | Opcode::Mpp | Opcode::End),
            [],
        ) => true,
        (
            Some(Opcode::Ld | Opcode::Ldi | Opcode::And | Opcode::Ani | Opcode::Or | Opcode::Ori),
            [
                Device {
                    device: X | Y | M | T | C,
                    ..
                },
            ],
        ) => true,
        (Some(Opcode::Out | Opcode::Set), [Device { device: Y | M, .. }]) => true,
        (
            Some(Opcode::Rst),
            [
                Device {
                    device: Y | M | D | T | C,
                    ..
                },
            ],
        ) => true,
        (Some(Opcode::Out), [Device { device: T | C, .. }, Device { device: D, .. }]) => true,
        (
            Some(Opcode::Out),
            [
                Device { device: T | C, .. },
                Constant {
                    value, radix: 10, ..
                },
            ],
        ) => (0..=32767).contains(value),
        _ => false,
    }
}

/// FX spelling only. Addresses are syntactically bounded, not CPU-range checked.
pub(crate) fn text_operand(text: &str) -> OperandValue {
    let text = text.to_ascii_uppercase();
    let Some((prefix, rest)) = text.as_bytes().split_first() else {
        return OperandValue::Unknown;
    };
    let Ok(digits) = std::str::from_utf8(rest) else {
        return OperandValue::Unknown;
    };
    let device = match prefix {
        b'X' => Some(DeviceKind::X),
        b'Y' => Some(DeviceKind::Y),
        b'M' => Some(DeviceKind::M),
        b'D' => Some(DeviceKind::D),
        b'T' => Some(DeviceKind::T),
        b'C' => Some(DeviceKind::C),
        _ => None,
    };
    if let Some(device) = device {
        let radix = if matches!(device, DeviceKind::X | DeviceKind::Y) {
            8
        } else {
            10
        };
        if !digits.is_empty()
            && digits.bytes().all(|b| b.is_ascii_digit())
            && let Ok(address) = u32::from_str_radix(digits, radix)
        {
            return OperandValue::Device {
                device,
                address,
                radix: radix as u8,
            };
        }
    } else if *prefix == b'K' {
        let unsigned = digits
            .strip_prefix('-')
            .or_else(|| digits.strip_prefix('+'))
            .unwrap_or(digits);
        if !unsigned.is_empty()
            && unsigned.bytes().all(|b| b.is_ascii_digit())
            && let Ok(value) = digits.parse::<i32>()
        {
            return OperandValue::Constant {
                value: value as i64,
                radix: 10,
                bit_width: None,
            };
        }
    } else if *prefix == b'H'
        && !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_hexdigit())
        && let Ok(value) = u32::from_str_radix(digits, 16)
    {
        return OperandValue::Constant {
            value: value as i64,
            radix: 16,
            bit_width: None,
        };
    }
    OperandValue::Unknown
}
