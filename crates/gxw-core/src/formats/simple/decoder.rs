use crate::{FramingStatus, GxwError, ParsedProject, SourceSpan, ir::*};
use std::sync::Arc;

fn merged(a: &SourceSpan, b: &SourceSpan) -> InstructionSpan {
    InstructionSpan {
        source_id: Some(a.source_id),
        offset: a.offset,
        length: b.offset + b.length - a.offset,
    }
}

fn operand(bytes: &[u8]) -> OperandValue {
    use DeviceKind::*;
    // Only compact forms actually represented in the GXW/CSV pair. In
    // particular, negative K encodings and T/C tags have no binary oracle yet.
    let kind = match bytes {
        [4, 0x9c, _, 4] => Some((X, 8)),
        [4, 0x9d, _, 4] => Some((Y, 8)),
        [4, 0x90, _, 4] | [5, 0x90, _, _, 5] => Some((M, 10)),
        [4, 0xa8, _, 4] => Some((D, 10)),
        _ => None,
    };
    let value = bytes[2..bytes.len() - 1]
        .iter()
        .rev()
        .fold(0u32, |v, b| (v << 8) | *b as u32);
    if let Some((device, radix)) = kind {
        return OperandValue::Device {
            device,
            address: value,
            radix,
        };
    }
    match bytes {
        [4, tag @ (0xe8 | 0xe9), value @ 0..=127, 4] => OperandValue::Constant {
            value: *value as i64,
            radix: 10,
            bit_width: Some(if *tag == 0xe8 { 16 } else { 32 }),
        },
        [5, 0xea, _, _, 5] => OperandValue::Constant {
            value: value as i64,
            radix: 16,
            bit_width: Some(16),
        },
        _ => OperandValue::Unknown,
    }
}

pub(crate) fn decode(
    project: Arc<ParsedProject>,
    index: usize,
    profile: DeviceProfile,
) -> Result<InstructionProgram, GxwError> {
    let raw = project
        .programs
        .get(index)
        .ok_or_else(|| GxwError::format("program index", "out of range"))?;
    let mut program = InstructionProgram {
        schema_version: 1,
        origin: "gxw".into(),
        source_sha256: project.index.sha256.clone(),
        device_profile: profile,
        logical_name: raw.logical_name.clone(),
        instructions: Vec::new(),
        diagnostics: Vec::new(),
        opaque_regions: Vec::new(),
        csv_rows: Vec::new(),
        complete: false,
        backing: InstructionBacking::Gxw(project.clone()),
    };
    let mut cursor = 0;
    while cursor < raw.tokens.len() {
        let start = cursor;
        let token = &raw.tokens[cursor];
        let bytes = project
            .source_bytes(&token.source)
            .expect("validated raw span");
        let Some((name, arity)) = super::catalog::entry(bytes) else {
            break;
        };
        cursor += 1;
        let mut operands = Vec::new();
        for _ in 0..arity {
            let Some(t) = raw.tokens.get(cursor) else {
                break;
            };
            let first = t;
            let b = project.source_bytes(&t.source).expect("validated raw span");
            // Only these two modifier records are established by the paired
            // CSV. Preserve the compound operand, without interpreting it.
            let modified = matches!(b, [4, 0xf0, 0, 4] | [4, 0xf1, 2, 4]);
            let (last, b) = if modified {
                let Some(next) = raw.tokens.get(cursor + 1) else {
                    break;
                };
                (
                    next,
                    project
                        .source_bytes(&next.source)
                        .expect("validated raw span"),
                )
            } else {
                (t, b)
            };
            // Refuse an opcode in an operand slot, rather than consuming it.
            if b.len() < 4 || b[1] < 0x80 {
                break;
            }
            operands.push(Operand {
                value: if modified {
                    OperandValue::Unknown
                } else {
                    operand(b)
                },
                original_text: None,
                encoding_width_bits: if modified {
                    None
                } else {
                    u8::try_from((b.len() - 3) * 8).ok()
                },
                source: merged(&first.source, &last.source),
            });
            cursor += if modified { 2 } else { 1 };
        }
        if operands.len() != arity {
            cursor = start;
            break;
        }
        program.instructions.push(Instruction {
            ordinal: program.instructions.len(),
            step: None,
            opcode: Opcode::from_mnemonic(name),
            mnemonic: if name == "@POINTER" {
                None
            } else {
                Some(name.into())
            },
            original_mnemonic: None,
            operands,
            source: merged(&token.source, &raw.tokens[cursor - 1].source),
            supported: false,
        });
    }
    if let Some(token) = raw.tokens.get(cursor) {
        let end = raw.token_region.as_ref().unwrap();
        let span = InstructionSpan {
            source_id: Some(token.source.source_id),
            offset: token.source.offset,
            length: end.offset + end.length - token.source.offset,
        };
        program.diagnostic(
            "GXW_INSTRUCTION_BOUNDARY_UNKNOWN",
            "Unverified opcode or operand boundary; remaining records are opaque",
            Some(span.clone()),
        );
        program.opaque_regions.push(span);
    }
    if raw.framing_status != FramingStatus::Complete {
        for region in &raw.opaque_regions {
            program.opaque_regions.push((&region.source).into());
        }
        program.diagnostic(
            "GXW_INSTRUCTION_FRAMING_INCOMPLETE",
            "Raw POU framing is incomplete; consult ParsedProject diagnostics",
            raw.source.as_ref().map(Into::into),
        );
    }
    program.finish(raw.framing_status == FramingStatus::Complete);
    Ok(program)
}
