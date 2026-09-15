//! GX Works2 list CSV (UTF-16LE BOM, quoted TSV), SH-080780ENG §6.16.
use crate::{GxwError, ReadLimits, ir::*};
use std::sync::Arc;

fn span(start: usize, end: usize) -> InstructionSpan {
    InstructionSpan {
        source_id: None,
        offset: start as u64,
        length: (end - start) as u64,
    }
}
fn bad(offset: usize, message: &str) -> GxwError {
    GxwError::format("GX Works2 CSV", format!("byte {offset}: {message}"))
}

fn rows(bytes: &[u8], limits: &ReadLimits) -> Result<Vec<CsvRow>, GxwError> {
    if !bytes.starts_with(&[0xff, 0xfe]) || !bytes.len().is_multiple_of(2) {
        return Err(bad(0, "expected UTF-16LE BOM and complete code units"));
    }
    let unit = |pos: usize| u16::from_le_bytes([bytes[pos], bytes[pos + 1]]);
    let mut pos = 2;
    let mut rows = Vec::new();
    while pos < bytes.len() {
        limits.check("CSV rows", rows.len() as u64 + 1, limits.max_entries)?;
        let start = pos;
        let mut fields = Vec::new();
        let mut field_sources = Vec::new();
        loop {
            if fields.len() >= 7 {
                return Err(bad(pos, "more than seven columns"));
            }
            let field_start = pos;
            if pos >= bytes.len() || unit(pos) != 34 {
                return Err(bad(pos, "expected quoted field"));
            }
            pos += 2;
            let mut value = Vec::new();
            loop {
                if pos >= bytes.len() {
                    return Err(bad(pos, "unterminated quoted field"));
                }
                let ch = unit(pos);
                pos += 2;
                if ch == 34 {
                    if pos < bytes.len() && unit(pos) == 34 {
                        value.push(34);
                        pos += 2;
                    } else {
                        break;
                    }
                } else {
                    if ch == 0 {
                        return Err(bad(pos - 2, "NUL character"));
                    }
                    value.push(ch);
                }
            }
            fields
                .push(String::from_utf16(&value).map_err(|_| bad(field_start, "invalid UTF-16"))?);
            field_sources.push(span(field_start, pos));
            if pos == bytes.len() {
                break;
            }
            match unit(pos) {
                9 => pos += 2,
                13 if pos + 2 < bytes.len() && unit(pos + 2) == 10 => {
                    pos += 4;
                    break;
                }
                _ => {
                    return Err(bad(
                        pos,
                        "expected tab, CRLF or end of file after closing quote",
                    ));
                }
            }
        }
        rows.push(CsvRow {
            number: rows.len() + 1,
            fields,
            field_sources,
            source: span(start, pos),
        });
    }
    Ok(rows)
}

pub(crate) fn parse(
    data: Arc<[u8]>,
    profile: DeviceProfile,
    limits: &ReadLimits,
) -> Result<InstructionProgram, GxwError> {
    limits.check("input bytes", data.len() as u64, limits.max_file_bytes)?;
    let csv_rows = rows(&data, limits)?;
    if csv_rows.len() < 3
        || csv_rows[0].fields.len() != 1
        || csv_rows[1].fields.len() != 2
        || csv_rows[2].fields.len() != 7
    {
        return Err(bad(
            2,
            "expected project, PC information and seven-column header rows",
        ));
    }
    if !matches!(
        csv_rows[2].fields[0].as_str(),
        "ステップ番号" | "Step No." | "Step No"
    ) || !matches!(csv_rows[2].fields[2].as_str(), "命令" | "Instruction")
    {
        return Err(GxwError::Unsupported {
            context: "CSV header".into(),
            message: "unrecognized column schema".into(),
        });
    }
    let mut program = InstructionProgram {
        schema_version: 1,
        origin: "csv".into(),
        source_sha256: crate::container::digest(&data),
        device_profile: profile,
        logical_name: Some(csv_rows[0].fields[0].clone()),
        instructions: Vec::new(),
        diagnostics: Vec::new(),
        opaque_regions: Vec::new(),
        csv_rows,
        complete: false,
        backing: InstructionBacking::Csv(data),
    };
    if !program.csv_rows[1].fields[1].starts_with("FXCPU ") {
        program.diagnostic(
            "GXW_CSV_CPU_UNVERIFIED",
            "PC information does not confirm the caller-selected FX device syntax",
            Some(program.csv_rows[1].source.clone()),
        );
    }
    let mut previous_step = None;
    let mut attach = false;
    for i in 3..program.csv_rows.len() {
        let row = &program.csv_rows[i];
        if row.fields.len() != 7 {
            return Err(bad(
                row.source.offset as usize,
                "instruction row must have seven columns",
            ));
        }
        let f = &row.fields;
        if !f[2].is_empty() {
            limits.check(
                "instructions",
                program.instructions.len() as u64 + 1,
                limits.max_tokens,
            )?;
            let step = if f[0].is_empty() {
                None
            } else {
                if !f[0].bytes().all(|b| b.is_ascii_digit()) {
                    return Err(bad(row.source.offset as usize, "invalid step number"));
                }
                Some(
                    f[0].parse::<u32>()
                        .map_err(|_| bad(row.source.offset as usize, "step number overflow"))?,
                )
            };
            if step
                .zip(previous_step)
                .is_some_and(|(next, last)| next <= last)
            {
                return Err(bad(
                    row.source.offset as usize,
                    "step numbers must increase",
                ));
            }
            if step.is_some() {
                previous_step = step;
            }
            let original = f[2].clone();
            program.instructions.push(Instruction {
                ordinal: program.instructions.len(),
                step,
                opcode: Opcode::from_mnemonic(&original),
                mnemonic: Some(original.to_ascii_uppercase()),
                original_mnemonic: Some(original),
                operands: Vec::new(),
                source: row.source.clone(),
                supported: false,
            });
            attach = true;
        } else if !f[0].is_empty() || !f[1].is_empty() || !f[5].is_empty() || !f[6].is_empty() {
            attach = false;
        }
        if !f[3].is_empty() {
            if !attach {
                return Err(bad(
                    row.source.offset as usize,
                    "orphan operand continuation",
                ));
            }
            let ins = program
                .instructions
                .last_mut()
                .ok_or_else(|| bad(row.source.offset as usize, "orphan operand"))?;
            // Seven arguments covers the current scope; larger forms remain a resource error.
            limits.check("operands per instruction", ins.operands.len() as u64 + 1, 7)?;
            ins.operands.push(Operand {
                value: text_operand(&f[3]),
                original_text: Some(f[3].clone()),
                encoding_width_bits: None,
                source: row.field_sources[3].clone(),
            });
            ins.source.length = row.source.offset + row.source.length - ins.source.offset;
        } else if f[2].is_empty() {
            attach = false;
        }
        if !f[4].is_empty() {
            program.diagnostic(
                "GXW_CSV_RESERVED_COLUMN",
                "Nonempty reserved column is not interpreted",
                Some(row.field_sources[4].clone()),
            );
        }
        let row = &program.csv_rows[i];
        if !row.fields[0].is_empty()
            && row.fields[2].is_empty()
            && row.fields[1].is_empty()
            && row.fields[5].is_empty()
            && row.fields[6].is_empty()
        {
            program.diagnostic(
                "GXW_CSV_ROW_UNSUPPORTED",
                "Step number without instruction or statement",
                Some(row.source.clone()),
            );
        }
    }
    program.finish(true);
    Ok(program)
}
