mod support;
use gxw_core::*;
use std::{path::PathBuf, sync::Arc};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/third_party/ladder_converter")
        .join(name)
}
fn csv(rows: &[&[&str]]) -> Vec<u8> {
    let mut text = "\"test\"\r\n\"PC情報:\"\t\"FXCPU FX3G\"\r\n\"ステップ番号\"\t\"行間ステートメント\"\t\"命令\"\t\"I/O(デバイス)\"\t\"空欄\"\t\"PIステートメント\"\t\"ノート\"\r\n".to_string();
    for row in rows {
        text.push_str(
            &row.iter()
                .map(|s| format!("\"{}\"", s.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join("\t"),
        );
        text.push_str("\r\n");
    }
    [
        vec![0xff, 0xfe],
        text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
    ]
    .concat()
}
fn parse(data: Vec<u8>) -> Result<InstructionProgram, GxwError> {
    parse_csv_bytes(data.into(), DeviceProfile::Fx, &ReadLimits::default())
}
fn binary(body: &[u8]) -> InstructionProgram {
    let data = support::raw_project(
        support::xml(&support::raw_row("7", "MAIN.pou")).as_bytes(),
        &[("/7", &support::pou(body))],
    );
    let project = Arc::new(parse_bytes(data.into(), &ReadLimits::default()).unwrap());
    decode_program(project, 0, DeviceProfile::Fx).unwrap()
}

#[test]
fn native_pair_matches_all_supported_opcodes_operands_order_and_boundaries() {
    let project = Arc::new(load_path(&fixture("FX2N.gxw"), &ReadLimits::default()).unwrap());
    assert_eq!(
        project.programs[0].profile.as_deref(),
        Some("simple-le-observed-v2-trailer24")
    );
    assert_eq!(project.programs[0].tokens.len(), 416);
    assert_eq!(project.programs[0].trailer.as_ref().unwrap().length, 24);
    let decoded = decode_program(project.clone(), 0, DeviceProfile::Fx).unwrap();
    let exported = load_csv(
        &fixture("MAIN.csv"),
        DeviceProfile::Fx,
        &ReadLimits::default(),
    )
    .unwrap();
    assert_eq!(decoded.instructions.len(), 170);
    assert_eq!(exported.instructions.len(), 170);
    assert!(!decoded.complete && !exported.complete);
    assert!(decoded.opaque_regions.is_empty());
    let alignment: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture("alignment.json")).unwrap()).unwrap();
    let mut opcodes = std::collections::BTreeSet::new();
    let mut supported = 0;
    for ((a, b), expected) in decoded
        .instructions
        .iter()
        .zip(&exported.instructions)
        .zip(alignment.as_array().unwrap())
    {
        assert_eq!(a.source.offset, expected["offset"].as_u64().unwrap());
        assert_eq!(a.source.length, expected["length"].as_u64().unwrap());
        assert!(decoded.source_bytes(&a.source).is_some());
        assert_eq!(a.supported, b.supported);
        if a.mnemonic.is_some() {
            assert_eq!(a.mnemonic, b.mnemonic);
        }
        if a.supported {
            supported += 1;
            opcodes.insert(a.opcode.unwrap().as_str());
            assert_eq!(a.opcode, b.opcode);
            assert_eq!(a.operands.len(), b.operands.len());
            for (x, y) in a.operands.iter().zip(&b.operands) {
                assert_eq!(x.value, y.value);
            }
        }
    }
    assert_eq!(supported, 83);
    assert_eq!(opcodes.len(), 15);
    // Full raw inspection remains its own, unchanged semantic contract.
    assert!(
        project
            .to_json()
            .unwrap()
            .contains("\"semantic_status\": \"not_decoded\"")
    );
    assert!(decode_program(project, 1, DeviceProfile::Fx).is_err());
}

#[test]
fn binary_preserves_unknown_specials_and_does_not_scan_ahead() {
    for unknown in [
        &[4, 0x24, 2, 4][..],
        &[3, 0xee, 3][..],
        &[4, 0xf1, 3, 4][..],
    ] {
        let mut body = vec![3, 0, 3, 4, 0x9c, 0, 4];
        body.extend_from_slice(unknown);
        body.extend_from_slice(&[3, 0x34, 3]);
        let p = binary(&body);
        assert!(!p.complete);
        assert_eq!(p.instructions.len(), 1);
        assert_eq!(p.opaque_regions[0].offset, 86);
        assert_eq!(p.source_bytes(&p.opaque_regions[0]).unwrap(), &body[7..]);
    }
    let p = binary(&[3, 0, 3, 3, 0x34, 3]); // END in operand slot.
    assert!(p.instructions.is_empty());
    assert_eq!(p.opaque_regions[0].offset, 79);
    let p = binary(&[3, 0, 3, 4, 0xc2, 0, 4, 3, 0x34, 3]);
    assert_eq!(p.instructions[0].operands[0].value, OperandValue::Unknown);
    assert!(!p.complete); // T/C wire tags are intentionally not guessed.
}

#[test]
fn output_followed_by_contacts_and_multiple_outputs_keep_sequence() {
    let p = binary(&[
        3, 0, 3, 4, 0x9c, 8, 4, 3, 0x20, 3, 4, 0x9d, 0, 4, 3, 0x0c, 3, 4, 0x90, 1, 4, 3, 0x20, 3,
        4, 0x9d, 1, 4, 3, 0x20, 3, 4, 0x9d, 2, 4, 3, 0x34, 3,
    ]);
    assert!(p.complete);
    assert_eq!(
        p.instructions
            .iter()
            .map(|i| i.opcode.unwrap().as_str())
            .collect::<Vec<_>>(),
        ["LD", "OUT", "AND", "OUT", "OUT", "END"]
    );
    assert_eq!(
        p.instructions[0].operands[0].value,
        OperandValue::Device {
            device: DeviceKind::X,
            address: 8,
            radix: 8
        }
    );
    assert!(!binary(&[3, 0x34, 3, 3, 0x34, 3]).complete);
}

#[test]
fn csv_preserves_unicode_quotes_tabs_newlines_and_original_byte_spans() {
    let p = parse(csv(&[
        &[
            "0",
            "日本語😀\t\"引用\"\r\n次行",
            "ld",
            "X010",
            "",
            "",
            "注記",
        ],
        &["1", "", "OUT", "Y001", "", "", ""],
        &["2", "", "END", "", "", "", ""],
    ]))
    .unwrap();
    assert!(p.complete);
    let i = &p.instructions[0];
    assert_eq!(i.original_mnemonic.as_deref(), Some("ld"));
    assert_eq!(i.mnemonic.as_deref(), Some("LD"));
    assert_eq!(p.csv_rows[3].fields[1], "日本語😀\t\"引用\"\r\n次行");
    assert_eq!(i.operands[0].original_text.as_deref(), Some("X010"));
    for row in &p.csv_rows {
        for (field, span) in row.fields.iter().zip(&row.field_sources) {
            let original = p.source_bytes(span).unwrap();
            let units = original
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect::<Vec<_>>();
            assert_eq!(
                String::from_utf16(&units).unwrap(),
                format!("\"{}\"", field.replace('"', "\"\""))
            );
        }
    }
}

#[test]
fn csv_constants_keep_sign_radix_and_unknown_width_without_truncation() {
    let cases = [
        ("K-32768", -32768, 10),
        ("K-2147483648", -2147483648, 10),
        ("K2147483647", 2147483647, 10),
        ("HFFFFFFFF", 4294967295, 16),
    ];
    for (text, value, radix) in cases {
        let p = parse(csv(&[
            &["0", "", "MOV", text, "", "", ""],
            &["", "", "", "D0", "", "", ""],
            &["5", "", "END", "", "", "", ""],
        ]))
        .unwrap();
        assert!(!p.complete); // MOV is outside the initial supported opcode set.
        assert_eq!(
            p.instructions[0].operands[0].value,
            OperandValue::Constant {
                value,
                radix,
                bit_width: None
            }
        );
        assert_eq!(p.instructions[0].operands.len(), 2);
    }
    for text in [
        "X008",
        "Y9",
        "M-1",
        "D4294967296",
        "K2147483648",
        "K-2147483649",
        "H100000000",
        "K2X000",
        "D100Z0",
        "label",
        "K",
        "H-1",
    ] {
        let p = parse(csv(&[
            &["0", "", "LD", text, "", "", ""],
            &["1", "", "END", "", "", "", ""],
        ]))
        .unwrap();
        assert_eq!(
            p.instructions[0].operands[0].value,
            OperandValue::Unknown,
            "{text}"
        );
        assert!(!p.complete);
    }
}

#[test]
fn csv_timer_counter_syntax_is_separate_from_unverified_binary_encoding() {
    for device in ["T0", "C0"] {
        let p = parse(csv(&[
            &["0", "", "LD", "X0", "", "", ""],
            &["1", "", "OUT", device, "", "", ""],
            &["", "", "", "K10", "", "", ""],
            &["4", "", "RST", device, "", "", ""],
            &["6", "", "END", "", "", "", ""],
        ]))
        .unwrap();
        assert!(p.complete);
        assert_eq!(p.instructions[1].operands.len(), 2);
    }
    for rows in [
        vec![
            &["0", "", "OUT", "T0", "", "", ""][..],
            &["3", "", "END", "", "", "", ""],
        ],
        vec![
            &["0", "", "LD", "D0", "", "", ""][..],
            &["1", "", "END", "", "", "", ""],
        ],
        vec![
            &["0", "", "OUT", "X0", "", "", ""][..],
            &["1", "", "END", "", "", "", ""],
        ],
        vec![
            &["0", "", "MPS", "X0", "", "", ""][..],
            &["1", "", "END", "", "", "", ""],
        ],
    ] {
        assert!(!parse(csv(&rows)).unwrap().complete);
    }
}

#[test]
fn csv_rejects_malformed_encoding_shape_steps_and_orphan_continuations() {
    let valid = csv(&[
        &["0", "", "LD", "X0", "", "", ""],
        &["1", "", "END", "", "", "", ""],
    ]);
    for n in 0..valid.len() {
        let result = parse(valid[..n].to_vec());
        if let Ok(p) = result {
            assert!(!p.complete || n >= valid.len() - 4);
        }
    }
    let mut invalid = valid.clone();
    invalid[0] = 0xfe;
    assert!(parse(invalid).is_err());
    let mut invalid = valid.clone();
    invalid[4..6].copy_from_slice(&0xd800u16.to_le_bytes());
    assert!(parse(invalid).is_err());
    for rows in [
        vec![
            &["0", "", "LD", "X0", "", "", ""][..],
            &["0", "", "END", "", "", "", ""],
        ],
        vec![&["4294967296", "", "LD", "X0", "", "", ""][..]],
        vec![&["", "", "", "D0", "", "", ""][..]],
        vec![&["0", "", "END", "", "", "", "", "extra"][..]],
        vec![&["0", "", "END"][..]],
    ] {
        assert!(parse(csv(&rows)).is_err());
    }
    for limits in [
        ReadLimits {
            max_file_bytes: 8,
            ..ReadLimits::default()
        },
        ReadLimits {
            max_entries: 3,
            ..ReadLimits::default()
        },
        ReadLimits {
            max_tokens: 1,
            ..ReadLimits::default()
        },
    ] {
        assert!(matches!(
            parse_csv_bytes(valid.clone().into(), DeviceProfile::Fx, &limits),
            Err(GxwError::ResourceLimit { .. })
        ));
    }
}

#[test]
fn csv_english_header_and_unsupported_rows_are_explicit() {
    let bytes = csv(&[&["0", "", "END", "", "", "", ""]]);
    let units = bytes[2..]
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect::<Vec<_>>();
    let english = String::from_utf16(&units)
        .unwrap()
        .replace("ステップ番号", "Step No.")
        .replace("命令", "Instruction");
    let encoded = [
        vec![0xff, 0xfe],
        english.encode_utf16().flat_map(u16::to_le_bytes).collect(),
    ]
    .concat();
    assert!(parse(encoded).unwrap().complete);
    let p = parse(csv(&[
        &["0", "", "", "", "", "", ""],
        &["1", "", "END", "", "", "", ""],
    ]))
    .unwrap();
    assert!(!p.complete);
    assert!(
        p.diagnostics
            .iter()
            .any(|d| d.code == "GXW_CSV_ROW_UNSUPPORTED")
    );
    let bytes = csv(&[&["0", "", "END", "", "reserved", "", ""]]);
    assert!(!parse(bytes).unwrap().complete);
}

#[test]
fn unknown_binary_constant_width_and_signed_forms_are_retained() {
    for constant in [
        &[5, 0xe8, 0xff, 0xff, 5][..],
        &[7, 0xe9, 0xff, 0xff, 0xff, 0xff, 7][..],
    ] {
        let mut body = vec![5, 0x4c, 5, 0, 5]; // Known MOV boundary, unsupported opcode.
        body.extend_from_slice(constant);
        body.extend_from_slice(&[4, 0xa8, 0, 4, 3, 0x34, 3]);
        let p = binary(&body);
        assert_eq!(p.instructions.len(), 2);
        assert_eq!(p.instructions[0].operands[0].value, OperandValue::Unknown);
        assert_eq!(
            p.source_bytes(&p.instructions[0].operands[0].source)
                .unwrap(),
            constant
        );
        assert!(!p.complete);
    }
}
