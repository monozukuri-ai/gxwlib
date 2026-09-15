use gxw_core::*;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}
fn load(name: &str) -> Arc<InstructionProgram> {
    Arc::new(load_csv(&fixture(name), DeviceProfile::Fx, &ReadLimits::default()).unwrap())
}
fn program(sequence: &str) -> Arc<InstructionProgram> {
    let mut text="\"test\"\r\n\"PC情報:\"\t\"FXCPU FX3G\"\r\n\"ステップ番号\"\t\"行間ステートメント\"\t\"命令\"\t\"I/O(デバイス)\"\t\"空欄\"\t\"PIステートメント\"\t\"ノート\"\r\n".to_owned();
    for (n, line) in sequence.split(';').enumerate() {
        let parts: Vec<_> = line.split_whitespace().collect();
        text.push_str(&format!(
            "\"{n}\"\t\"\"\t\"{}\"\t\"{}\"\t\"\"\t\"\"\t\"\"\r\n",
            parts[0],
            parts.get(1).unwrap_or(&"")
        ));
        for op in parts.iter().skip(2) {
            text.push_str(&format!("\"\"\t\"\"\t\"\"\t\"{op}\"\t\"\"\t\"\"\t\"\"\r\n"));
        }
    }
    let mut bytes = vec![255, 254];
    bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    Arc::new(parse_csv_bytes(bytes.into(), DeviceProfile::Fx, &ReadLimits::default()).unwrap())
}
fn compiled(p: Arc<InstructionProgram>) -> Arc<CompiledProgram> {
    Arc::new(compile_program(p, SimulationProfile::Fx3g, 100_000).unwrap())
}
fn simulator(p: Arc<InstructionProgram>) -> Simulator {
    Simulator::new(compiled(p), 10_000_000, &[], SimulationLimits::default()).unwrap()
}
fn values(v: &serde_json::Value) -> Vec<(String, bool)> {
    v.as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_bool().unwrap()))
        .collect()
}
fn is_code(e: GxwError, code: &str) {
    assert!(
        matches!(&e,GxwError::Simulation{code:c,..} if c==code),
        "{e}"
    );
}

#[test]
fn independent_state_oracles_cover_latches_order_writes_and_saved_reads() {
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture("simulation/scenarios.json")).unwrap())
            .unwrap();
    let mut count = 0;
    for case in cases["cases"].as_array().unwrap() {
        let p = load(&format!("simulation/{}", case["file"].as_str().unwrap()));
        let mut s = Simulator::new(
            compiled(p),
            17,
            &values(&case["initial"]),
            SimulationLimits::default(),
        )
        .unwrap();
        for (n, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            s.set_inputs(&values(&step["inputs"])).unwrap();
            let snapshot = s.step(true).unwrap();
            assert_eq!(snapshot.scan, n as u64 + 1);
            assert_eq!(snapshot.time_ns, (n as u64 + 1) * 17);
            for (device, value) in values(&step["expected"]) {
                assert_eq!(
                    snapshot.get(&device).unwrap(),
                    value,
                    "{} scan {} {device}",
                    case["name"],
                    n + 1
                );
                if device.starts_with('Y') {
                    assert_eq!(snapshot.output(&device).unwrap(), value);
                }
            }
            for event in &snapshot.instruction_trace {
                assert!(
                    snapshot
                        .compiled()
                        .program()
                        .source_bytes(&event.source)
                        .is_some()
                );
            }
            count += 1;
        }
    }
    assert_eq!(count, 29);
}

#[test]
fn printed_circuit_oracles_exhaust_all_input_combinations() {
    let mut combinations = 0;
    for name in ["or_blocks", "and_blocks", "saved_results", "cascade_blocks"] {
        let p = compiled(load(&format!("ladder/{name}.csv")));
        let expected: serde_json::Value = serde_json::from_slice(
            &std::fs::read(fixture(&format!("ladder/{name}.json"))).unwrap(),
        )
        .unwrap();
        let inputs: Vec<_> = p
            .devices()
            .iter()
            .filter(|d| d.device == DeviceKind::X)
            .map(DeviceRef::name)
            .collect();
        let mut s = Simulator::new(p.clone(), 1, &[], SimulationLimits::default()).unwrap();
        for bits in 0..(1 << inputs.len()) {
            let values: BTreeMap<_, _> = inputs
                .iter()
                .enumerate()
                .map(|(i, n)| (n.clone(), bits & (1 << i) != 0))
                .collect();
            s.set_inputs(&values.clone().into_iter().collect::<Vec<_>>())
                .unwrap();
            let result = s.step(false).unwrap();
            for out in expected["outputs"].as_array().unwrap() {
                // Paths/polarities were manually transcribed from printed circuits,
                // independently of the VM and the reconstructed ladder graph.
                let want = out["paths"].as_array().unwrap().iter().any(|path| {
                    path.as_array().unwrap().iter().all(|contact| {
                        let index = contact[0].as_u64().unwrap() as usize;
                        let name = p.instructions()[index].device.as_ref().unwrap().name();
                        values[&name] != contact[1].as_bool().unwrap()
                    })
                });
                assert_eq!(
                    result.output(out["device"].as_str().unwrap()).unwrap(),
                    want,
                    "{name} inputs={bits}"
                );
            }
            combinations += 1;
        }
    }
    assert_eq!(combinations, 1232);
}

#[test]
fn input_latch_and_output_refresh_are_distinct_from_mid_scan_image_writes() {
    let mut s = simulator(load("simulation/double_coil.csv"));
    s.set_inputs(&[("X1".into(), true), ("X2".into(), false)])
        .unwrap();
    assert!(!s.snapshot().get("X1").unwrap());
    let r = s.step(true).unwrap();
    assert!(r.get("X1").unwrap());
    assert!(!r.output("Y3").unwrap());
    assert!(r.output("Y4").unwrap());
    assert_eq!(r.instruction_trace[1].write_value, Some(true));
    assert_eq!(r.instruction_trace[1].external_output, Some(false));
    assert_eq!(r.instruction_trace[2].read_value, Some(true));
    assert_eq!(r.instruction_trace[2].external_output, Some(false));
    assert_eq!(r.instruction_trace[5].write_value, Some(false));
    s.set_devices(&[("Y3".into(), true)]).unwrap();
    assert!(s.snapshot().get("Y3").unwrap());
    assert!(!s.snapshot().output("Y3").unwrap());
    assert!(!r.get("Y3").unwrap()); // independent snapshot
}

#[test]
fn compiler_rejects_unknown_devices_incomplete_code_and_invalid_structure() {
    for seq in [
        "LD X0;OUT T192 K10;END",
        "LD T246;OUT Y0;END",
        "LD C200;OUT Y0;END",
        "LD M8000;OUT Y0;END",
        "LD X200;OUT Y0;END",
        "LD X0;OUT Y200;END",
        "LD X0;RST D0;END",
        "LD M7680;OUT Y0;END",
    ] {
        is_code(
            compile_program(program(seq), SimulationProfile::Fx3g, 1000).unwrap_err(),
            "GXW_SIM_DEVICE_UNSUPPORTED",
        );
    }
    for seq in [
        "LD X0;MOV D0 D1;END",
        "LD label;OUT Y0;END",
        "LD X0;OUT Y0",
        "LD X0;END;OUT Y0;END",
    ] {
        is_code(
            compile_program(program(seq), SimulationProfile::Fx3g, 1000).unwrap_err(),
            "GXW_SIM_INCOMPLETE",
        );
    }
    for seq in [
        "AND X0;OUT Y0;END",
        "LD X0;ANB;OUT Y0;END",
        "LD X0;MPP;OUT Y0;END",
        "LD X0;MPS;OUT Y0;END",
        "LD X0;LD X1;OUT Y0;END",
    ] {
        is_code(
            compile_program(program(seq), SimulationProfile::Fx3g, 1000).unwrap_err(),
            "GXW_SIM_STRUCTURE_INVALID",
        );
    }
    is_code(
        SimulationProfile::parse("q").unwrap_err(),
        "GXW_SIM_PROFILE_UNSUPPORTED",
    );
    let mut p = program("LD X0;OUT Y0;END");
    Arc::get_mut(&mut p).unwrap().instructions[0].source.offset = u64::MAX;
    is_code(
        compile_program(p, SimulationProfile::Fx3g, 100).unwrap_err(),
        "GXW_SIM_SOURCE_INVALID",
    );
    let mut p = program("LD X0;OUT Y0;END");
    Arc::get_mut(&mut p).unwrap().instructions[0].ordinal = 999;
    is_code(
        compile_program(p, SimulationProfile::Fx3g, 100).unwrap_err(),
        "GXW_SIM_INSTRUCTION_UNSUPPORTED",
    );
    assert!(matches!(
        compile_program(program("END"), SimulationProfile::Fx3g, 0),
        Err(GxwError::ResourceLimit { .. })
    ));
}

#[test]
fn batch_run_matches_steps_and_snapshot_reset_are_independent() {
    let p = program("LDI M0;OUT M0;LD M0;OUT Y0;END");
    let mut a = simulator(p.clone());
    let mut b = simulator(p);
    let watch = vec!["m00".into(), "Y0".into()];
    let trace = a.run(20, Some(&watch)).unwrap();
    assert_eq!(trace.watch, ["M0", "Y0"]);
    assert_eq!(trace.completed_scans, 20);
    assert!(!trace.interrupted);
    for sample in &trace.samples {
        let r = b.step(false).unwrap();
        assert_eq!(sample.scan, r.scan);
        assert_eq!(sample.time_ns, r.time_ns);
        assert_eq!(
            sample.values,
            vec![r.get("M0").unwrap(), r.get("Y0").unwrap()]
        );
    }
    assert_eq!(
        trace.final_snapshot.to_json().unwrap(),
        b.snapshot().to_json().unwrap()
    );
    let before = a.step(true).unwrap();
    assert!(before.get("M0").unwrap());
    a.reset(&[]).unwrap();
    assert_eq!(a.scan_count(), 0);
    assert_eq!(a.time_ns(), 0);
    assert!(!a.snapshot().get("M0").unwrap());
    assert!(before.get("M0").unwrap());
    assert!(!trace.final_snapshot.get("M0").unwrap());
    assert_eq!(a.run(0, Some(&[])).unwrap().completed_scans, 0);
}

#[test]
fn invalid_updates_limits_and_time_overflow_leave_state_unchanged() {
    let mut s = simulator(program("LD X0;OUT Y0;END"));
    s.set_inputs(&[("X0".into(), true)]).unwrap();
    assert!(
        s.set_inputs(&[("X0".into(), false), ("X200".into(), true)])
            .is_err()
    );
    assert!(
        s.set_inputs(&[("X0".into(), false), ("x00".into(), true)])
            .is_err()
    );
    assert!(
        s.set_devices(&[("M0".into(), true), ("M8000".into(), true)])
            .is_err()
    );
    assert!(s.set_inputs(&[("M0".into(), true)]).is_err());
    assert!(s.set_devices(&[("X0".into(), false)]).is_err());
    assert!(s.step(false).unwrap().get("X0").unwrap());
    assert!(!s.snapshot().get("M0").unwrap());
    let before = s.snapshot().to_json().unwrap();
    assert!(s.run(100_001, None).is_err());
    assert!(s.run(1, Some(&["Y0".into(), "y00".into()])).is_err());
    assert!(s.reset(&[("M8000".into(), true)]).is_err());
    assert_eq!(s.snapshot().to_json().unwrap(), before);
    let p = compiled(program("LD X0;OUT Y0;END"));
    let mut s = Simulator::new(p.clone(), u64::MAX, &[], SimulationLimits::default()).unwrap();
    assert!(s.run(2, None).is_err());
    assert_eq!(s.scan_count(), 0);
    s.step(false).unwrap();
    let before = s.snapshot().to_json().unwrap();
    is_code(s.step(false).unwrap_err(), "GXW_SIM_TIME_OVERFLOW");
    assert_eq!(s.snapshot().to_json().unwrap(), before);
    let mut s = Simulator::new(
        p.clone(),
        1,
        &[],
        SimulationLimits {
            max_events: 0,
            max_trace_values: 1,
            max_operations: 9,
            ..SimulationLimits::default()
        },
    )
    .unwrap();
    assert!(s.step(true).is_err());
    assert!(s.run(1, None).is_err());
    assert!(s.run(4, Some(&[])).is_err());
    assert_eq!(s.scan_count(), 0);
    assert!(Simulator::new(p, 0, &[], SimulationLimits::default()).is_err());
}

#[test]
fn cancellation_reports_completed_scans_and_can_resume() {
    let mut s = simulator(program("LDI M0;OUT M0;END"));
    let mut checks = 0;
    let t = s
        .run_interruptible(1000, Some(&["M0".into()]), || {
            checks += 1;
            checks == 3
        })
        .unwrap();
    assert!(t.interrupted);
    assert_eq!(t.completed_scans, 512);
    assert_eq!(t.samples.len(), 512);
    assert_eq!(s.scan_count(), 512);
    assert_eq!(s.step(false).unwrap().scan, 513);
}

#[test]
fn nested_saved_results_and_stack_capacity_match_boolean_oracles() {
    let mut s = simulator(program(
        "LD X0;MPS;AND X1;MPS;AND X2;OUT Y0;MRD;AND X3;OUT Y1;MPP;OUT Y2;MPP;OUT Y3;END",
    ));
    for bits in 0..16 {
        let input: Vec<_> = (0..4)
            .map(|i| (format!("X{i}"), bits & (1 << i) != 0))
            .collect();
        s.set_inputs(&input).unwrap();
        let r = s.step(false).unwrap();
        assert_eq!(r.output("Y0").unwrap(), bits & 7 == 7);
        assert_eq!(r.output("Y1").unwrap(), bits & 11 == 11);
        assert_eq!(r.output("Y2").unwrap(), bits & 3 == 3);
        assert_eq!(r.output("Y3").unwrap(), bits & 1 == 1);
    }
    for operator in ["ANB", "ORB"] {
        let sequence = (0..8).map(|i| format!("LD X{i};")).collect::<String>()
            + &format!("{}OUT Y0;END", format!("{operator};").repeat(7));
        let mut s = simulator(program(&sequence));
        for bits in 0..256 {
            s.set_inputs(
                &(0..8)
                    .map(|i| (format!("X{i}"), bits & (1 << i) != 0))
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            assert_eq!(
                s.step(false).unwrap().output("Y0").unwrap(),
                if operator == "ANB" {
                    bits == 255
                } else {
                    bits != 0
                }
            );
        }
    }
    let sequence = format!(
        "LD M0;{}RST M0;{}END",
        "MPS;".repeat(11),
        "MPP;OUT Y0;".repeat(11)
    );
    let mut s = simulator(program(&sequence));
    s.set_devices(&[("M0".into(), true)]).unwrap();
    let r = s.step(false).unwrap();
    assert!(!r.get("M0").unwrap());
    assert!(r.output("Y0").unwrap());
    for bad in [
        format!("LD X0;{}OUT Y0;END", "MPS;".repeat(12)),
        format!("{}OUT Y0;END", "LD X0;".repeat(9)),
    ] {
        is_code(
            compile_program(program(&bad), SimulationProfile::Fx3g, 1000).unwrap_err(),
            "GXW_SIM_STRUCTURE_INVALID",
        );
    }
    let mut s = simulator(program(&format!(
        "{}LD X1;ANB;OUT Y1;END",
        "LD X0;OUT Y0;".repeat(30)
    )));
    s.set_inputs(&[("X0".into(), true), ("X1".into(), false)])
        .unwrap();
    let r = s.step(false).unwrap();
    assert!(r.output("Y0").unwrap());
    assert!(!r.output("Y1").unwrap());
}
