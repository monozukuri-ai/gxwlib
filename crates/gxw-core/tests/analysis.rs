use gxw_core::*;
use std::{path::PathBuf, sync::Arc};
fn program(sequence: &str) -> Arc<InstructionProgram> {
    let mut text = "\"test\"\r\n\"PC情報:\"\t\"FXCPU FX3G\"\r\n\"ステップ番号\"\t\"行間ステートメント\"\t\"命令\"\t\"I/O(デバイス)\"\t\"空欄\"\t\"PIステートメント\"\t\"ノート\"\r\n".to_string();
    for (n, line) in sequence.split(';').enumerate() {
        let parts: Vec<_> = line.split_whitespace().collect();
        text.push_str(&format!(
            "\"{n}\"\t\"\"\t\"{}\"\t\"{}\"\t\"\"\t\"\"\t\"\"\r\n",
            parts[0],
            parts.get(1).unwrap_or(&"")
        ));
        for operand in parts.iter().skip(2) {
            text.push_str(&format!(
                "\"\"\t\"\"\t\"\"\t\"{operand}\"\t\"\"\t\"\"\t\"\"\r\n"
            ));
        }
    }
    let bytes = [
        vec![0xff, 0xfe],
        text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
    ]
    .concat();
    Arc::new(parse_csv_bytes(bytes.into(), DeviceProfile::Fx, &ReadLimits::default()).unwrap())
}
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/ladder")
        .join(name)
}

#[test]
fn xref_matches_manual_sites_and_external_writers_are_configurable() {
    let p = program("LD X010;OUT M0;LD M0;SET Y0;LD X1;RST Y0;LD M1;OUT Y1;LD M2;OUT Y1;END");
    let options = AnalysisOptions {
        external_writes: vec![DeviceRef::parse("M1").unwrap()],
        ..AnalysisOptions::default()
    };
    let a = analyze(p.clone(), &options).unwrap();
    assert!(a.complete);
    let sites: Vec<_> = a
        .accesses
        .iter()
        .map(|a| (a.device.name(), a.instruction, a.mode))
        .collect();
    assert_eq!(
        sites,
        vec![
            ("X10".into(), 0, AccessMode::Read),
            ("M0".into(), 1, AccessMode::Write),
            ("M0".into(), 2, AccessMode::Read),
            ("Y0".into(), 3, AccessMode::Write),
            ("X1".into(), 4, AccessMode::Read),
            ("Y0".into(), 5, AccessMode::Write),
            ("M1".into(), 6, AccessMode::Read),
            ("Y1".into(), 7, AccessMode::Write),
            ("M2".into(), 8, AccessMode::Read),
            ("Y1".into(), 9, AccessMode::Write)
        ]
    );
    let multiple: Vec<_> = a
        .diagnostics
        .iter()
        .filter(|d| d.code == "GXW_MULTIPLE_WRITES")
        .collect();
    assert_eq!(multiple.len(), 1);
    assert_eq!(multiple[0].related_instructions, [7, 9]);
    let missing: Vec<_> = a
        .diagnostics
        .iter()
        .filter(|d| d.code == "GXW_NO_KNOWN_LOCAL_WRITE")
        .collect();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].instruction, Some(8));
    assert_eq!(missing[0].severity, "info");
    for access in &a.accesses {
        assert!(p.source_bytes(&access.source).is_some());
    }
    assert!(DeviceRef::parse("X8").is_err());
    let raw_inputs = analyze(
        p,
        &AnalysisOptions {
            inputs_external: false,
            ..AnalysisOptions::default()
        },
    )
    .unwrap();
    assert!(
        raw_inputs
            .diagnostics
            .iter()
            .any(|d| d.instruction == Some(0) && d.code == "GXW_NO_KNOWN_LOCAL_WRITE")
    );
}

#[test]
fn unresolved_effects_and_preset_reads_are_never_guessed() {
    let p = program("LD X0;OUT T0 D0;RST T0;MOV D0 D1;LD label;OUT Y0;END");
    let a = analyze(p, &AnalysisOptions::default()).unwrap();
    assert!(!a.complete);
    let modes: Vec<_> = a
        .accesses
        .iter()
        .map(|a| (a.instruction, a.operand, a.mode))
        .collect();
    assert!(modes.contains(&(1, 0, AccessMode::Write)));
    assert!(modes.contains(&(1, 1, AccessMode::Read)));
    assert!(modes.contains(&(3, 0, AccessMode::Unknown)));
    assert!(modes.contains(&(3, 1, AccessMode::Unknown)));
    assert!(
        a.diagnostics
            .iter()
            .any(|d| d.code == "GXW_OPERAND_UNRESOLVED")
    );
    assert!(
        a.accesses
            .iter()
            .any(|a| a.instruction == 5 && a.mode == AccessMode::Write)
    ); // syntactic references beyond unknown flow.
}

#[test]
fn official_printed_circuit_paths_polarities_and_outputs_match() {
    for name in ["or_blocks", "and_blocks", "saved_results", "cascade_blocks"] {
        let p = Arc::new(
            load_csv(
                &fixture(&format!("{name}.csv")),
                DeviceProfile::Fx,
                &ReadLimits::default(),
            )
            .unwrap(),
        );
        let graph = Arc::new(build_ladder(p.clone(), &AnalysisOptions::default()).unwrap());
        assert!(graph.complete, "{name}: {:?}", graph.diagnostics);
        let expected: serde_json::Value =
            serde_json::from_slice(&std::fs::read(fixture(&format!("{name}.json"))).unwrap())
                .unwrap();
        let mut paths = Vec::<Vec<Vec<(usize, bool)>>>::new();
        for condition in &graph.conditions {
            let value = match condition.kind {
                ConditionKind::Contact => vec![vec![(condition.instruction, condition.inverted)]],
                ConditionKind::And => paths[condition.inputs[0]]
                    .iter()
                    .flat_map(|a| {
                        paths[condition.inputs[1]]
                            .iter()
                            .map(move |b| a.iter().chain(b).copied().collect())
                    })
                    .collect(),
                ConditionKind::Or => paths[condition.inputs[0]]
                    .iter()
                    .chain(&paths[condition.inputs[1]])
                    .cloned()
                    .collect(),
            };
            paths.push(value);
        }
        assert_eq!(
            graph.outputs.len(),
            expected["outputs"].as_array().unwrap().len()
        );
        for (output, want) in graph
            .outputs
            .iter()
            .zip(expected["outputs"].as_array().unwrap())
        {
            assert_eq!(
                output.instruction,
                want["instruction"].as_u64().unwrap() as usize
            );
            assert_eq!(
                ladder::render::operand_label(&p.instructions[output.instruction].operands[0]),
                want["device"].as_str().unwrap()
            );
            assert_eq!(
                serde_json::to_value(&paths[output.condition]).unwrap(),
                want["paths"],
                "{name} output #{}",
                output.instruction
            );
        }
        let svg = render_svg(graph, &RenderOptions::default()).unwrap();
        assert!(svg.complete);
        for e in &svg.elements {
            assert_eq!(e.source, p.instructions[e.instruction].source);
            assert!(e.x + e.width <= svg.width);
            assert!(e.y + e.height <= svg.height);
        }
        assert_eq!(
            svg.svg.matches("class=\"gxw-symbol").count(),
            svg.elements.len()
        );
    }
}

#[test]
fn reads_reused_after_writes_keep_original_instruction_identity() {
    let p = program("LD X0;OUT M0;AND M0;OUT Y0;MPS;RST M0;MPP;OUT Y1;END");
    let graph = Arc::new(build_ladder(p, &AnalysisOptions::default()).unwrap());
    assert!(graph.complete);
    assert_eq!(graph.outputs[1].condition, graph.outputs[2].condition);
    assert_eq!(graph.outputs[1].condition, graph.outputs[3].condition);
    assert_eq!(
        graph
            .conditions
            .iter()
            .filter(|n| n.kind == ConditionKind::Contact)
            .map(|n| n.instruction)
            .collect::<Vec<_>>(),
        [0, 2]
    );
    let svg = render_svg(graph, &RenderOptions::default()).unwrap();
    assert_eq!(
        svg.elements
            .iter()
            .filter(|e| e.kind == "contact" && e.instruction == 2)
            .count(),
        3
    );
}

#[test]
fn stack_errors_partial_input_and_resource_limits_are_explicit() {
    for (sequence, code) in [
        ("AND X0;OUT Y0;END", "GXW_LOGIC_UNDERFLOW"),
        ("LD X0;ANB;OUT Y0;END", "GXW_BLOCK_UNDERFLOW"),
        ("LD X0;MPP;OUT Y0;END", "GXW_MPS_UNDERFLOW"),
        ("LD X0;MPS;OUT Y0;END", "GXW_MPS_UNCLOSED"),
        ("LD X0;LD X1;OUT Y0;END", "GXW_BLOCK_UNCLOSED"),
        (
            "LD X0;OUT Y0;MOV K0 D0;LD X1;OUT Y1;END",
            "GXW_LADDER_UNSUPPORTED",
        ),
    ] {
        let g = build_ladder(program(sequence), &AnalysisOptions::default()).unwrap();
        assert!(!g.complete);
        assert!(
            g.diagnostics.iter().any(|d| d.code == code),
            "{code}: {:?}",
            g.diagnostics
        );
    }
    let mps = program(&format!("LD X0;{}OUT Y0;END", "MPS;".repeat(12)));
    assert!(
        build_ladder(mps, &AnalysisOptions::default())
            .unwrap()
            .diagnostics
            .iter()
            .any(|d| d.code == "GXW_MPS_OVERFLOW")
    );
    let ld = program(&format!("{}OUT Y0;END", "LD X0;".repeat(9)));
    assert!(
        build_ladder(ld, &AnalysisOptions::default())
            .unwrap()
            .diagnostics
            .iter()
            .any(|d| d.code == "GXW_BLOCK_OVERFLOW")
    );
    // Many independent rungs do not accumulate unmatched logic-stack errors.
    let many = program(&format!("{}END", "LD X0;OUT Y0;".repeat(100)));
    let graph = Arc::new(build_ladder(many.clone(), &AnalysisOptions::default()).unwrap());
    assert!(graph.complete);
    assert!(matches!(
        analyze(
            many,
            &AnalysisOptions {
                max_instructions: 2,
                ..AnalysisOptions::default()
            }
        ),
        Err(GxwError::ResourceLimit { .. })
    ));
    assert!(matches!(
        render_svg(
            graph,
            &RenderOptions {
                max_elements: 5,
                ..RenderOptions::default()
            }
        ),
        Err(GxwError::ResourceLimit { .. })
    ));
    let graph = Arc::new(build_ladder(program("END"), &AnalysisOptions::default()).unwrap());
    assert!(matches!(
        render_svg(
            graph,
            &RenderOptions {
                max_output_bytes: 5,
                ..RenderOptions::default()
            }
        ),
        Err(GxwError::ResourceLimit { .. })
    ));
}

#[test]
fn diff_aligns_insert_delete_replace_and_retains_both_source_ranges() {
    let a = program("LD X000;OUT Y0;END");
    let b = program("ld X0;OUT Y0;END");
    let same = diff_programs(a.clone(), b, 0).unwrap();
    assert!(same.complete && !same.different);
    let b = program("LD X0;AND M0;OUT Y0;END");
    let d = diff_programs(a.clone(), b.clone(), 100).unwrap();
    assert!(d.different && d.complete);
    assert_eq!(
        d.hunks.iter().map(|h| h.kind).collect::<Vec<_>>(),
        [DiffKind::Equal, DiffKind::Insert, DiffKind::Equal]
    );
    assert_eq!(d.hunks[1].right_start, 1);
    assert_eq!(d.hunks[1].right_count, 1);
    let d = diff_programs(b, a.clone(), 100).unwrap();
    assert_eq!(d.hunks[1].kind, DiffKind::Delete);
    let d = diff_programs(a, program("LD X1;OUT Y0;END"), 100).unwrap();
    assert_eq!(d.hunks[0].kind, DiffKind::Replace);
    let unknown = program("LD label;OUT Y0;END");
    let same = diff_programs(unknown.clone(), unknown, 10).unwrap();
    assert!(!same.complete && !same.different);
    assert!(matches!(
        diff_programs(program("LD X0;OUT Y0;END"), program("LD X1;OUT Y1;END"), 1),
        Err(GxwError::ResourceLimit { .. })
    ));
}

#[test]
fn renderer_rejects_invalid_graphs_and_limits_shared_expansion() {
    for kind in 0..5 {
        let mut g = build_ladder(program("LD X0;OUT Y0;END"), &AnalysisOptions::default()).unwrap();
        match kind {
            0 => g.outputs[0].instruction = usize::MAX,
            1 => g.controls[0].instruction = usize::MAX,
            2 => g.conditions[0].inputs.push(0),
            3 => g.conditions[0].id = 9,
            _ => {
                g.conditions[0].kind = ConditionKind::And;
                g.conditions[0].inputs = vec![0, 0];
            }
        }
        assert!(render_svg(Arc::new(g), &RenderOptions::default()).is_err());
    }
    let mut g = build_ladder(program("LD X0;OUT Y0;END"), &AnalysisOptions::default()).unwrap();
    for id in 1..30 {
        g.conditions.push(Condition {
            id,
            kind: ConditionKind::And,
            instruction: 0,
            inputs: vec![id - 1, id - 1],
            inverted: false,
        });
    }
    g.outputs[0].condition = 29;
    assert!(matches!(
        render_svg(Arc::new(g), &RenderOptions::default()),
        Err(GxwError::ResourceLimit { .. })
    ));
    let p = program(&format!("LD X0;{}OUT Y0;END", "AND X1;".repeat(2000)));
    assert!(
        render_svg(
            Arc::new(build_ladder(p, &AnalysisOptions::default()).unwrap()),
            &RenderOptions::default()
        )
        .is_ok()
    );
}

#[test]
fn html_user_text_is_escaped_and_not_a_template() {
    let mut p = program("LD X0;OUT Y0;END");
    Arc::get_mut(&mut p).unwrap().logical_name =
        Some("{{DATA}}{{SVG}}</script><script>window.attacked=1</script>&\"".into());
    let doc = render_svg(
        Arc::new(build_ladder(p, &AnalysisOptions::default()).unwrap()),
        &RenderOptions::default(),
    )
    .unwrap();
    let html = doc.to_html().unwrap();
    assert!(html.contains("{{DATA}}{{SVG}}&lt;/script&gt;"));
    assert!(!html.contains("<script>window.attacked"));
    assert!(html.contains("\\u003c/script>"));
    assert_eq!(html.matches("<svg ").count(), 1);
}

#[test]
fn opaque_only_changes_are_reported_outside_instruction_hunks() {
    let mut a = program("ld X0;OUT Y0;END");
    let mut b = program("LD X0;OUT Y0;END");
    for p in [&mut a, &mut b] {
        let p = Arc::get_mut(p).unwrap();
        p.complete = false;
        // Model retained, uninterpreted bytes independently of decoded keys.
        p.opaque_regions.push(p.instructions[0].source.clone());
    }
    let diff = diff_programs(a, b, 100).unwrap();
    assert!(!diff.complete && diff.different && diff.opaque_changed);
    assert!(diff.hunks.iter().all(|h| h.kind == DiffKind::Equal));
}
