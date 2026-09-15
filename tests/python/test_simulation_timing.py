import json
import subprocess
import sys
from pathlib import Path

import pytest
from test_analysis import program

import gxwlib

FIXTURES = Path(__file__).resolve().parents[1] / "fixtures/timing"
CASES = json.loads((FIXTURES / "scenarios.json").read_text())["cases"]


def simulator(sequence, period=100_000_000, **kwargs):
    c = gxwlib.compile_program(program(sequence), profile="fx3g")
    return gxwlib.Simulator(c, scan_period_ns=period, **kwargs)


def subset(actual, expected):
    if isinstance(expected, dict):
        for key, value in expected.items():
            subset(actual[key] if isinstance(actual, dict) else getattr(actual, key), value)
    else:
        assert actual == expected


@pytest.mark.parametrize("case", CASES, ids=lambda c: c["name"])
def test_fixed_timing_expectations(case):
    p = gxwlib.load_csv(FIXTURES / case["file"], device_profile="fx")
    c = gxwlib.compile_program(p, profile="fx3g")
    s = gxwlib.Simulator(
        c,
        scan_period_ns=case["period_ns"],
        timer_phase_ns=case["phase_ns"],
        initial_state=case["initial"],
    )
    for action in case["actions"]:
        match action["op"]:
            case "step":
                s.set_inputs(action["inputs"])
                snap = s.step(trace_instructions=True)
            case "advance_stopped":
                s.advance_stopped(action["elapsed_ns"])
                snap = s.snapshot()
            case "period":
                s.set_scan_period_ns(action["period_ns"])
                snap = s.snapshot()
            case op:
                getattr(s, op)()
                snap = s.snapshot()
        subset(snap, action["expected"])
        subset(json.loads(snap.to_json()), action["expected"])
        for name, t in snap.timers.items():
            assert isinstance(t, gxwlib.TimerState)
            assert t.done == snap.get(name) and t.value == snap.timer(name).value
        for name, counter in snap.counters.items():
            assert isinstance(counter, gxwlib.CounterState)
            assert counter.done == snap.get(name) and counter.value == snap.counter(name).value


def test_numeric_trace_and_source_views_survive_reset_and_owner_deletion():
    a = simulator("LD X1;RST T250;RST C16;LD X0;OUT T250 K2;OUT C16 K2;END")
    b = gxwlib.Simulator(a.compiled, scan_period_ns=a.scan_period_ns)
    for s in (a, b):
        s.set_inputs({"X0": True})
    trace = a.run(4, watch=["t0250", "C016", "Y0"])
    assert trace.watch == ["T250", "C16", "Y0"]
    for sample in trace.samples:
        snap = b.step()
        assert sample.timers["T250"].value == snap.timer("T250").value
        assert sample.counters["C16"].value == snap.counter("C16").value
        assert sample.values == [snap.get(d) for d in trace.watch]
    snapshot = a.step(trace_instructions=True)
    event = snapshot.instruction_trace[4]
    state = event.timer
    source = snapshot.compiled.program.read_source(event.source)
    assert event.opcode == "OUT" and state.done and state.preset == 2
    assert snapshot.compiled.instructions[4].preset == 2
    assert snapshot.instruction_trace[5].counter.value == 1
    a.reset()
    assert not a.snapshot().timer("T250").done
    with pytest.raises(AttributeError):
        state.value = 0
    snapshot.timers.clear()
    assert snapshot.timer("T250").done
    del a, b, snapshot
    assert trace.final_snapshot.timer("T250").value == 2
    assert state.value == 2 and event.source.length == len(source)


def test_explicit_scope_invalid_presets_and_numeric_update_rejection():
    for sequence, code in [
        ("LD X0;OUT T246 K1;END", "GXW_SIM_DEVICE_UNSUPPORTED"),
        ("LD X0;OUT C200 K1;END", "GXW_SIM_DEVICE_UNSUPPORTED"),
        ("LD X0;OUT T0 D0;END", "GXW_SIM_PRESET_UNSUPPORTED"),
        ("LD X0;OUT C0 D0;END", "GXW_SIM_PRESET_UNSUPPORTED"),
        ("LD X0;OUT T0 K1;OUT T0 K2;END", "GXW_SIM_MULTIPLE_COIL"),
        ("LD X0;RST C0;RST C0;END", "GXW_SIM_MULTIPLE_RESET"),
    ]:
        with pytest.raises(gxwlib.SimulationError) as e:
            simulator(sequence)
        assert e.value.code == code and e.value.instruction is not None
    s = simulator("LD X0;OUT T0 K1;OUT C0 K1;END")
    for operation in [
        lambda: s.set_devices({"M0": True, "T0": True}),
        lambda: s.set_devices({"C0": False}),
        lambda: s.reset(initial_state={"T0": False}),
        lambda: s.set_scan_period_ns(0),
        lambda: s.advance_stopped(1),
    ]:
        before = s.snapshot().to_json()
        with pytest.raises(gxwlib.SimulationError):
            operation()
        assert s.snapshot().to_json() == before
    with pytest.raises(gxwlib.SimulationError, match="phase"):
        gxwlib.Simulator(s.compiled, scan_period_ns=1, timer_phase_ns=100_000_000)
    with pytest.raises(gxwlib.SimulationError):
        s.snapshot().timer("M0")
    with pytest.raises(gxwlib.SimulationError):
        s.snapshot().counter("T0")


def test_stopped_time_trace_limits_and_large_integer_arithmetic():
    s = simulator("LD X0;OUT T319 K32767;OUT C199 K32767;END", timer_phase_ns=99_999_999)
    s.set_inputs({"X0": True})
    s.step()
    s.stop()
    before = s.snapshot().to_json()
    for operation in [s.step, lambda: s.run(0), lambda: s.advance_stopped(2**64 - 1)]:
        with pytest.raises(gxwlib.SimulationError):
            operation()
        assert s.snapshot().to_json() == before
    s.advance_stopped(2**63)
    assert s.time_ns == 2**63 + 100_000_000
    s.start()
    assert s.step().counter("C199").value == 1  # Retained ON does not produce an edge.
    limited = gxwlib.Simulator(
        s.compiled, scan_period_ns=1, limits=gxwlib.SimulationLimits(max_trace_values=15)
    )
    with pytest.raises(gxwlib.ResourceLimitError):
        limited.run(1, watch=["T319"])
    assert limited.scan_count == 0
    empty = limited.run(1, watch=[])
    assert empty.samples[0].timers == empty.samples[0].counters == {}
    assert s.snapshot().counter("C0").preset is None


def test_cli_emits_numeric_states_and_rejects_invalid_phase():
    args = [
        sys.executable,
        "-m",
        "gxwlib",
        "simulate",
        str(FIXTURES / "bases.csv"),
        "--input-format",
        "csv",
        "--profile",
        "fx3g",
        "--scan-period-ns",
        "7500000",
        "--timer-phase-ns",
        "500000",
        "--input",
        "X0=1",
        "--scans",
        "5",
        "--watch",
        "T200",
    ]
    result = subprocess.run(args, capture_output=True, text=True, check=False)
    assert result.returncode == 0, result.stderr
    trace = json.loads(result.stdout)
    assert [s["timers"]["T200"]["value"] for s in trace["samples"]] == [0, 0, 1, 2, 3]
    assert trace["timer_phase_ns"] == 500000 and trace["schema_version"] == 2
    for phase in ["100000000", "-1", str(2**64)]:
        result = subprocess.run(
            [*args, "--timer-phase-ns", phase], capture_output=True, text=True, check=False
        )
        assert result.returncode == 1 and "Traceback" not in result.stderr
