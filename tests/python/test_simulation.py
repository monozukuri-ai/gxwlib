import gc
import json
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import pytest
from test_analysis import program

import gxwlib

FIXTURES = Path(__file__).resolve().parents[1] / "fixtures"
CASES = json.loads((FIXTURES / "simulation/scenarios.json").read_text())["cases"]


def simulator(sequence="LDI M0;OUT M0;LD M0;OUT Y0;END", **kwargs):
    compiled = gxwlib.compile_program(program(sequence), profile="fx3g")
    return gxwlib.Simulator(compiled, scan_period_ns=17, **kwargs)


@pytest.mark.parametrize("case", CASES, ids=lambda c: c["name"])
def test_independent_scan_oracles(case):
    p = gxwlib.load_csv(FIXTURES / "simulation" / case["file"], device_profile="fx")
    c = gxwlib.compile_program(p, profile="fx3g")
    s = gxwlib.Simulator(c, scan_period_ns=17, initial_state=case["initial"])
    assert c.profile == "fx3g" and c.execution_scope == "isolated_single_program"
    for n, step in enumerate(case["steps"], 1):
        s.set_inputs(step["inputs"])
        r = s.step(trace_instructions=True)
        assert r.scan == n and r.time_ns == n * 17
        assert {d: r.get(d) for d in step["expected"]} == step["expected"]
        for event in r.instruction_trace:
            assert p.read_source(event.source) == p.read_source(
                p.instructions[event.instruction].source
            )
            assert event.opcode == p.instructions[event.instruction].opcode


def test_latching_intermediate_writes_and_committed_outputs():
    p = gxwlib.load_csv(FIXTURES / "simulation/double_coil.csv", device_profile="fx")
    c = gxwlib.compile_program(p, profile="fx3g")
    s = gxwlib.Simulator(c, scan_period_ns=1)
    s.set_inputs({"X1": True, "X2": False})
    assert not s.snapshot().get("X1")
    r = s.step(trace_instructions=True)
    assert r.get("X1") and r.output("Y4") and not r.output("Y3")
    assert r.instruction_trace[1].write_value is True
    assert r.instruction_trace[2].read_value is True
    assert r.instruction_trace[1].external_output is False
    assert r.instruction_trace[2].external_output is False
    assert r.instruction_trace[5].write_value is False
    s.set_devices({"Y3": True})
    assert s.snapshot().get("Y3") and not s.snapshot().output("Y3")
    assert not r.get("Y3")


def test_run_matches_step_and_snapshots_keep_owned_sources():
    a, b = simulator(), simulator()
    trace = a.run(20, watch=["m00", "Y0"])
    assert trace.watch == ["M0", "Y0"]
    assert trace.completed_scans == trace.requested_scans == 20
    assert not trace.interrupted and trace.start_scan == 0
    for sample in trace.samples:
        r = b.step()
        assert sample.scan == r.scan and sample.time_ns == r.time_ns
        assert sample.values == [r.get("M0"), r.output("Y0")]
    assert json.loads(trace.final_snapshot.to_json()) == json.loads(b.snapshot().to_json())
    saved = a.step(trace_instructions=True)
    event = saved.instruction_trace[0]
    source_bytes = saved.compiled.program.read_source(event.source)
    with pytest.raises(ValueError, match="another"):
        b.compiled.program.read_source(event.source)
    a.reset()
    assert a.scan_count == a.time_ns == 0
    assert not a.snapshot().get("M0") and saved.get("M0")
    saved.devices["M0"] = False  # Returned maps are copies.
    assert saved.get("M0")
    with pytest.raises(AttributeError):
        saved.scan = 100
    del a, b, saved
    gc.collect()
    assert trace.final_snapshot.get("M0") is False and event.read_value is False
    # The event retains its snapshot, compiled program and original input buffer.
    assert event.source.length == len(source_bytes)
    assert trace.final_snapshot.compiled.program.instructions[0].opcode == "LDI"


def test_compile_preconditions_and_device_ranges():
    p = program("LD X0;OUT Y0;END")
    with pytest.raises(TypeError):
        gxwlib.compile_program(p)
    with pytest.raises(TypeError):
        gxwlib.Simulator(p, scan_period_ns=1)
    with pytest.raises(TypeError):
        gxwlib.Simulator(gxwlib.compile_program(p, profile="fx3g"))
    for text, code in [
        ("LD X0;MOV D0 D1;END", "GXW_SIM_INCOMPLETE"),
        ("LD X0;OUT T192 K10;END", "GXW_SIM_DEVICE_UNSUPPORTED"),
        ("LD X0;RST D0;END", "GXW_SIM_DEVICE_UNSUPPORTED"),
        ("LD M8000;OUT Y0;END", "GXW_SIM_DEVICE_UNSUPPORTED"),
        ("LD M7680;OUT Y0;END", "GXW_SIM_DEVICE_UNSUPPORTED"),
        ("LD X200;OUT Y0;END", "GXW_SIM_DEVICE_UNSUPPORTED"),
        ("LD X0;MPP;OUT Y0;END", "GXW_SIM_STRUCTURE_INVALID"),
    ]:
        with pytest.raises(gxwlib.SimulationError) as error:
            gxwlib.compile_program(program(text), profile="fx3g")
        assert error.value.code == code
    c = gxwlib.compile_program(program("LD X177;OUT M7679;LD M7679;OUT Y177;END"), profile="fx3g")
    s = gxwlib.Simulator(c, scan_period_ns=1)
    s.set_inputs({"x0177": True})
    assert s.step().output("Y177")
    with pytest.raises(gxwlib.ResourceLimitError):
        gxwlib.compile_program(p, profile="fx3g", max_instructions=1)
    with pytest.raises(gxwlib.SimulationError) as error:
        gxwlib.compile_program(p, profile="q")
    assert error.value.code == "GXW_SIM_PROFILE_UNSUPPORTED"


def test_atomic_input_state_updates_limits_and_time_overflow():
    s = simulator("LD X0;OUT Y0;END")
    s.set_inputs({"X0": True})
    for values in [{"X0": False, "X200": True}, {"X0": False, "x00": True}, {"M0": True}]:
        with pytest.raises(gxwlib.SimulationError):
            s.set_inputs(values)
    for values in [{"X0": 1}, {"X0": "true"}, {"X0": None}]:
        with pytest.raises(TypeError):
            s.set_inputs(values)
    assert s.step().output("Y0")
    previous = s.snapshot().to_json()
    for call in [
        lambda: s.set_devices({"M0": True, "M8000": True}),
        lambda: s.reset(initial_state={"X0": True}),
        lambda: s.run(100_001),
        lambda: s.run(2, watch=["Y0", "Y00"]),
        lambda: s.run(1, watch=["D0"]),
    ]:
        with pytest.raises(gxwlib.GxwError):
            call()
        assert s.snapshot().to_json() == previous
    limited = simulator(
        limits=gxwlib.SimulationLimits(max_events=0, max_operations=5, max_trace_values=1)
    )
    for call in [
        lambda: limited.step(trace_instructions=True),
        lambda: limited.run(2, watch=[]),
        lambda: limited.run(1, watch=["M0", "Y0"]),
    ]:
        with pytest.raises(gxwlib.ResourceLimitError):
            call()
        assert limited.scan_count == 0
    overflow = gxwlib.Simulator(s.compiled, scan_period_ns=2**64 - 1)
    with pytest.raises(gxwlib.SimulationError):
        overflow.run(2)
    assert overflow.scan_count == 0
    overflow.step()
    with pytest.raises(gxwlib.SimulationError) as error:
        overflow.step()
    assert error.value.code == "GXW_SIM_TIME_OVERFLOW" and overflow.scan_count == 1
    with pytest.raises(gxwlib.SimulationError):
        gxwlib.Simulator(s.compiled, scan_period_ns=0)


def test_same_instance_concurrent_operations_rejected_and_separate_instances_independent():
    slow = simulator(
        "LD X0;" + "AND X0;" * 2048 + "OUT Y0;END",
        limits=gxwlib.SimulationLimits(max_operations=300_000_000),
    )
    with ThreadPoolExecutor(max_workers=2) as pool:
        future = pool.submit(slow.run, 100_000, watch=[])
        deadline = time.monotonic() + 5
        while not slow.busy and not future.done() and time.monotonic() < deadline:
            time.sleep(0.001)
        assert slow.busy, "background run must hold the instance lock"
        for call in [
            lambda: slow.set_inputs({"X0": True}),
            lambda: slow.set_devices({"M0": True}),
            slow.step,
            slow.snapshot,
            slow.reset,
            lambda: slow.run(1),
            lambda: slow.scan_count,
            lambda: slow.time_ns,
            lambda: slow.scan_period_ns,
            lambda: slow.running,
            lambda: slow.timer_phase_ns,
            lambda: slow.set_scan_period_ns(1),
            slow.stop,
            slow.start,
            lambda: slow.advance_stopped(1),
        ]:
            with pytest.raises(gxwlib.SimulationError) as error:
                call()
            assert error.value.code == "GXW_SIM_BUSY"
        assert future.result(timeout=60).completed_scans == 100_000
        assert not slow.busy and slow.scan_count == 100_000
        a = simulator()
        b = gxwlib.Simulator(a.compiled, scan_period_ns=17, initial_state={"M0": True})
        futures = [pool.submit(s.run, 1000, watch=["M0"]) for s in [a, b]]
        left, right = [f.result(timeout=10) for f in futures]
        assert all(
            x.values[0] != y.values[0] for x, y in zip(left.samples, right.samples, strict=True)
        )


def test_python_interrupt_returns_completed_trace_and_unlocks_instance():
    # Isolate KeyboardInterrupt from the pytest process and verify normal shutdown.
    script = """
import _thread,threading,time
import gxwlib
from pathlib import Path
p = gxwlib.load_csv(SOURCE, device_profile="fx")
s = gxwlib.Simulator(gxwlib.compile_program(p,profile="fx3g"),scan_period_ns=1_000_000,
    limits=gxwlib.SimulationLimits(max_operations=300_000_000))
s.set_inputs({"X0": True})
def interrupt():
    deadline=time.monotonic()+5
    while not s.busy and time.monotonic()<deadline: time.sleep(.001)
    if s.busy:
        time.sleep(.02)
        _thread.interrupt_main()
t = threading.Thread(target=interrupt)
t.start()
try:
    s.run(100_000,watch=["M0"])
except KeyboardInterrupt as error:
    trace=error.trace
    assert trace.interrupted and 0 < trace.completed_scans < trace.requested_scans
    assert error.completed_scans == s.scan_count == len(trace.samples)
    assert trace.final_snapshot.get("M0") == bool(s.scan_count % 2)
    assert trace.final_snapshot.time_ns == s.time_ns == 1_000_000*s.scan_count
    assert trace.final_snapshot.timer("T256").value == min(s.scan_count-1,32767)
    assert trace.final_snapshot.counter("C16").value == min((s.scan_count+1)//2,32767)
    assert not s.busy
    assert s.step().scan == trace.completed_scans+1
    print("interrupt trace and resume passed")
else:
    raise AssertionError("expected interruption")
finally:
    t.join()
"""
    # Use a long valid chain so interruption occurs inside run even in release builds.
    import tempfile

    from test_instructions import csv_data

    with tempfile.TemporaryDirectory() as temporary:
        path = Path(temporary) / "interrupt.csv"
        ops = [
            "LDI M0",
            "OUT M0",
            "OUT C16 K32767",
            "LD X0",
            "OUT T256 K32767",
            *(["AND X0"] * 2048),
            "OUT Y0",
            "END",
        ]
        rows = []
        for n, line in enumerate(ops):
            parts = line.split()
            rows.append((n, parts[0], parts[1] if len(parts) > 1 else ""))
            rows.extend(("", "", operand) for operand in parts[2:])
        path.write_bytes(csv_data(rows))
        result = subprocess.run(
            [sys.executable, "-c", "SOURCE=" + repr(str(path)) + "\n" + script],
            capture_output=True,
            text=True,
            timeout=60,
            check=False,
        )
    assert result.returncode == 0, result.stdout + result.stderr
    assert "interrupt trace and resume passed" in result.stdout


def test_simulation_cli():
    path = FIXTURES / "simulation/double_coil.csv"
    base = [
        sys.executable,
        "-m",
        "gxwlib",
        "simulate",
        str(path),
        "--input-format",
        "csv",
        "--profile",
        "fx3g",
        "--scan-period-ns",
        "17",
    ]
    result = subprocess.run(
        [
            *base,
            "--input",
            "X1=1",
            "--input",
            "X2=0",
            "--scans",
            "3",
            "--watch",
            "Y3",
            "--watch",
            "Y4",
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 0, result.stderr
    trace = json.loads(result.stdout)
    assert trace["completed_scans"] == 3 and trace["samples"][0]["values"] == [False, True]
    for args in [
        ["--scans", "-1"],
        ["--input", "X1=bad"],
        ["--input", "X1=1", "--input", "x01=0"],
        ["--watch", "T246"],
        ["--scan-period-ns", "0"],
    ]:
        failed = subprocess.run([*base, *args], capture_output=True, text=True, check=False)
        assert failed.returncode == 1 and "Traceback" not in failed.stderr
