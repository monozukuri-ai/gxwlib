use gxw_core::*;
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/timing")
        .join(name)
}
fn program(sequence: &str) -> Arc<InstructionProgram> {
    let mut text = "\"timing\"\r\n\"PC情報:\"\t\"FXCPU FX3G\"\r\n\"ステップ番号\"\t\"行間ステートメント\"\t\"命令\"\t\"I/O(デバイス)\"\t\"空欄\"\t\"PIステートメント\"\t\"ノート\"\r\n".to_owned();
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
fn sim(seq: &str, period: u64, phase: u64) -> Simulator {
    let c = Arc::new(compile_program(program(seq), SimulationProfile::Fx3g, 100_000).unwrap());
    Simulator::with_timer_phase(c, period, &[], SimulationLimits::default(), phase).unwrap()
}
fn pairs(v: &Value) -> Vec<(String, bool)> {
    v.as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_bool().unwrap()))
        .collect()
}
fn subset(actual: &Value, expected: &Value, path: &str) {
    if let Some(map) = expected.as_object() {
        for (key, value) in map {
            subset(&actual[key], value, &format!("{path}.{key}"));
        }
    } else {
        assert_eq!(actual, expected, "{path}");
    }
}

#[test]
fn fixed_independent_scan_expectations_include_timing_edges_reset_and_stop_run() {
    let data: Value =
        serde_json::from_slice(&std::fs::read(fixture("scenarios.json")).unwrap()).unwrap();
    let mut actions = 0;
    for case in data["cases"].as_array().unwrap() {
        let p = Arc::new(
            load_csv(
                &fixture(case["file"].as_str().unwrap()),
                DeviceProfile::Fx,
                &ReadLimits::default(),
            )
            .unwrap(),
        );
        let c = Arc::new(compile_program(p, SimulationProfile::Fx3g, 100_000).unwrap());
        let mut s = Simulator::with_timer_phase(
            c,
            case["period_ns"].as_u64().unwrap(),
            &pairs(&case["initial"]),
            SimulationLimits::default(),
            case["phase_ns"].as_u64().unwrap(),
        )
        .unwrap();
        for (n, action) in case["actions"].as_array().unwrap().iter().enumerate() {
            let snap = match action["op"].as_str().unwrap() {
                "step" => {
                    s.set_inputs(&pairs(&action["inputs"])).unwrap();
                    s.step(true).unwrap()
                }
                "stop" => {
                    s.stop();
                    s.snapshot()
                }
                "start" => {
                    s.start();
                    s.snapshot()
                }
                "period" => {
                    s.set_scan_period_ns(action["period_ns"].as_u64().unwrap())
                        .unwrap();
                    s.snapshot()
                }
                "advance_stopped" => {
                    s.advance_stopped(action["elapsed_ns"].as_u64().unwrap())
                        .unwrap();
                    s.snapshot()
                }
                "reset" => {
                    s.reset(&[]).unwrap();
                    s.snapshot()
                }
                _ => panic!("invalid fixture operation"),
            };
            subset(
                &serde_json::to_value(&snap).unwrap(),
                &action["expected"],
                &format!("{} action {n}", case["name"]),
            );
            for t in snap.timers.values() {
                assert_eq!(snap.get(&t.device).unwrap(), t.done);
            }
            for c in snap.counters.values() {
                assert_eq!(snap.get(&c.device).unwrap(), c.done);
            }
            actions += 1;
        }
    }
    assert_eq!(actions, 71);
}

#[test]
fn timer_clock_matches_independent_pulse_enumeration_and_variable_scan_periods() {
    // Oracle enumerates oscillator pulse timestamps, not the VM's quotient formula.
    for (device, base) in [
        ("T0", 100_000_000),
        ("T200", 10_000_000),
        ("T256", 1_000_000),
    ] {
        for phase in [0, 1, base / 2, base - 1] {
            let seq = format!("LD X0;OUT {device} K7;END");
            let mut s = sim(&seq, base / 3, phase);
            s.set_inputs(&[("X0".into(), true)]).unwrap();
            let pulses: Vec<u64> = (1..=100).map(|n| n * base - phase).collect();
            for period in [
                base / 3,
                base / 3,
                base - 1,
                1,
                base + 1,
                base * 3,
                base * 2,
                1,
            ] {
                let evaluation_time = s.time_ns();
                s.set_scan_period_ns(period).unwrap();
                let snap = s.step(true).unwrap();
                let expected = pulses
                    .iter()
                    .filter(|p| **p <= evaluation_time)
                    .count()
                    .min(7) as u16;
                assert_eq!(
                    snap.timer(device).unwrap().value,
                    expected,
                    "{device}, phase={phase}, t={evaluation_time}"
                );
                assert_eq!(snap.get(device).unwrap(), expected == 7);
                assert_eq!(snap.instruction_trace[1].time_ns, evaluation_time);
                assert_eq!(snap.time_ns, evaluation_time + period);
            }
        }
    }
}

#[test]
fn counter_edges_saturate_and_reset_does_not_recount_a_held_input() {
    let mut s = sim("LD X1;RST C199;LD X0;OUT C199 K32767;END", 1, 0);
    // Alternating inputs allow exactly 32767 independently counted rising edges.
    for n in 0..32770 {
        s.set_inputs(&[("X0".into(), true)]).unwrap();
        assert_eq!(
            s.step(false).unwrap().counter("C199").unwrap().value,
            (n + 1).min(32767)
        );
        s.set_inputs(&[("X0".into(), false)]).unwrap();
        assert_eq!(
            s.step(false).unwrap().counter("C199").unwrap().value,
            (n + 1).min(32767)
        );
    }
    s.set_inputs(&[("X0".into(), true), ("X1".into(), true)])
        .unwrap();
    assert_eq!(s.step(false).unwrap().counter("C199").unwrap().value, 0);
    s.set_inputs(&[("X1".into(), false)]).unwrap();
    assert_eq!(
        s.run(10, None)
            .unwrap()
            .final_snapshot
            .counter("C199")
            .unwrap()
            .value,
        0
    );
}

#[test]
fn compile_rejects_unverified_schedules_dynamic_presets_and_repeated_coils() {
    for (seq, code) in [
        ("LD X0;OUT T192 K1;END", "GXW_SIM_DEVICE_UNSUPPORTED"),
        ("LD T199;OUT Y0;END", "GXW_SIM_DEVICE_UNSUPPORTED"),
        ("LD T246;OUT Y0;END", "GXW_SIM_DEVICE_UNSUPPORTED"),
        ("LD T249;OUT Y0;END", "GXW_SIM_DEVICE_UNSUPPORTED"),
        ("LD T320;OUT Y0;END", "GXW_SIM_DEVICE_UNSUPPORTED"),
        ("LD C200;OUT Y0;END", "GXW_SIM_DEVICE_UNSUPPORTED"),
        ("LD X0;OUT C235 K1;END", "GXW_SIM_DEVICE_UNSUPPORTED"),
        ("LD X0;OUT T0 D0;END", "GXW_SIM_PRESET_UNSUPPORTED"),
        ("LD X0;OUT C0 D0;END", "GXW_SIM_PRESET_UNSUPPORTED"),
        ("LD X0;OUT T0 K1;OUT T0 K2;END", "GXW_SIM_MULTIPLE_COIL"),
        ("LD X0;OUT C0 K1;OUT C0 K1;END", "GXW_SIM_MULTIPLE_COIL"),
        ("LD X0;RST T0;RST T0;END", "GXW_SIM_MULTIPLE_RESET"),
        ("LD X0;OUT T0 K-1;END", "GXW_SIM_INCOMPLETE"),
        ("LD X0;OUT C0 K32768;END", "GXW_SIM_INCOMPLETE"),
    ] {
        let e = compile_program(program(seq), SimulationProfile::Fx3g, 1000).unwrap_err();
        assert!(
            matches!(e,GxwError::Simulation{code:ref c,..} if c==code),
            "{seq}: {e}"
        );
    }
    for (d, base, retentive) in [
        ("T0", 100_000_000, false),
        ("T191", 100_000_000, false),
        ("T200", 10_000_000, false),
        ("T245", 10_000_000, false),
        ("T250", 100_000_000, true),
        ("T255", 100_000_000, true),
        ("T256", 1_000_000, false),
        ("T319", 1_000_000, false),
    ] {
        let mut s = sim(
            &format!("LD X0;OUT {d} K32767;END"),
            u64::MAX - 1,
            99_999_999,
        );
        s.set_inputs(&[("X0".into(), true)]).unwrap();
        let first = s.step(false).unwrap().timer(d).unwrap();
        assert_eq!(
            (first.time_base_ns, first.retentive, first.value),
            (base, retentive, 0)
        );
        s.set_scan_period_ns(1).unwrap();
        let final_state = s.step(false).unwrap();
        let t = final_state.timer(d).unwrap();
        assert_eq!(final_state.time_ns, u64::MAX);
        assert_eq!((t.value, t.elapsed_ns, t.done), (32767, 32767 * base, true));
    }
}

#[test]
fn reset_after_coil_preserves_earlier_reads_and_releases_in_instruction_order() {
    for (device, preset) in [("T250", 0), ("C16", 1)] {
        let seq = format!(
            "LD X0;OUT {device} K{preset};LD {device};OUT Y0;LD X1;RST {device};LD {device};OUT Y1;END"
        );
        let mut s = sim(&seq, 100_000_000, 0);
        s.set_inputs(&[("X0".into(), true)]).unwrap();
        s.step(false).unwrap();
        s.set_inputs(&[("X1".into(), true)]).unwrap();
        let snap = s.step(true).unwrap();
        assert!(snap.output("Y0").unwrap());
        assert!(!snap.output("Y1").unwrap());
        assert_eq!(snap.instruction_trace[2].read_value, Some(true));
        assert_eq!(snap.instruction_trace[6].read_value, Some(false));
        s.set_inputs(&[("X1".into(), false)]).unwrap();
        let release = s.step(true).unwrap();
        assert!(!release.output("Y0").unwrap());
        assert!(!release.get(device).unwrap());
        assert_eq!(release.instruction_trace[5].write_value, None);
        if device.starts_with('C') {
            assert!(!release.counter(device).unwrap().reset_active);
            assert_eq!(
                s.run(5, None)
                    .unwrap()
                    .final_snapshot
                    .counter(device)
                    .unwrap()
                    .value,
                0
            );
        } else {
            assert!(!release.timer(device).unwrap().reset_active);
            assert!(!s.step(false).unwrap().get(device).unwrap());
            assert!(s.step(false).unwrap().get(device).unwrap());
        }
    }
}

#[test]
fn numeric_trace_limits_stop_overflow_and_reset_are_atomic() {
    let mut s = sim("LD X0;OUT T0 K1;OUT C0 K1;END", 100_000_000, 99_999_999);
    s.set_inputs(&[("X0".into(), true)]).unwrap();
    let old = s.step(true).unwrap();
    let initial = s.snapshot().to_json().unwrap();
    assert!(
        s.set_devices(&[("M0".into(), true), ("T0".into(), true)])
            .is_err()
    );
    assert!(s.set_scan_period_ns(0).is_err());
    assert!(s.advance_stopped(1).is_err());
    assert_eq!(s.snapshot().to_json().unwrap(), initial);
    let trace = s.run(2, Some(&["T0".into(), "C0".into()])).unwrap();
    assert_eq!(trace.samples[0].timers["T0"].value, 1);
    assert_eq!(trace.samples[0].counters["C0"].value, 1);
    assert_eq!(trace.samples[0].values, vec![true, true]);
    assert!(!old.timer("T0").unwrap().done);
    assert!(old.timer("M0").is_err());
    assert!(old.counter("T0").is_err());
    assert_eq!(old.timer("T319").unwrap().preset, None);
    assert_eq!(old.counter("C199").unwrap().preset, None);
    s.stop();
    assert!(s.step(false).is_err());
    assert!(s.run(0, None).is_err());
    let stopped = s.snapshot().to_json().unwrap();
    assert!(s.advance_stopped(u64::MAX).is_err());
    assert_eq!(s.snapshot().to_json().unwrap(), stopped);
    s.set_scan_period_ns(19).unwrap();
    s.reset(&[]).unwrap();
    assert!(s.running());
    assert_eq!(
        (
            s.scan_count(),
            s.time_ns(),
            s.scan_period_ns(),
            s.timer_phase_ns()
        ),
        (0, 0, 19, 99_999_999)
    );
    let mut limited = Simulator::new(
        s.compiled().clone(),
        1,
        &[],
        SimulationLimits {
            max_trace_values: 15,
            ..SimulationLimits::default()
        },
    )
    .unwrap();
    assert!(matches!(
        limited.run(1, Some(&["T0".into()])),
        Err(GxwError::ResourceLimit { .. })
    ));
    assert_eq!(limited.scan_count(), 0);
    assert!(
        limited.run(1, Some(&[])).unwrap().samples[0]
            .timers
            .is_empty()
    );
    assert!(
        Simulator::with_timer_phase(
            s.compiled().clone(),
            1,
            &[],
            SimulationLimits::default(),
            100_000_000
        )
        .is_err()
    );
}
