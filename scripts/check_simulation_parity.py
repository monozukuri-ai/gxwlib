"""Compare compilation, per-instruction events and scan traces with standalone Rust."""

import argparse
import json
import subprocess
import tempfile
from pathlib import Path

import gxwlib


def attributes(value, shape):
    if isinstance(shape, dict):
        return {
            k: attributes(value[k] if isinstance(value, dict) else getattr(value, k), v)
            for k, v in shape.items()
        }
    if isinstance(shape, list):
        assert len(value) == len(shape)
        return [attributes(a, b) for a, b in zip(value, shape, strict=True)]
    return value


def verify(value, expected):
    assert attributes(value, expected) == json.loads(value.to_json()) == expected


def run(root, rust):
    config = {
        "period_ns": 17,
        "initial_state": {"M5": True},
        "inputs": [
            {"X0": True, "X1": False, "X2": False},
            {"X0": False, "X1": True, "X2": True},
            {"X0": True, "X1": True, "X2": False},
        ],
        "run_scans": 5,
        "watch": ["X0", "X1", "X2", "Y0", "Y1", "M0", "M5"],
    }
    records = []
    files = [p for p in sorted(root.rglob("*")) if p.suffix.lower() in {".gxw", ".csv"}]
    with tempfile.TemporaryDirectory(prefix="gxw-simulation-parity-") as temporary:
        config_path = Path(temporary) / "config.json"
        config_path.write_text(json.dumps(config), encoding="utf-8")
        for path in files:
            if path.suffix.lower() == ".csv":
                programs = [gxwlib.load_csv(path, device_profile="fx")]
            else:
                raw = gxwlib.load_project(path)
                programs = [
                    gxwlib.decode_program(raw, i, device_profile="fx")
                    for i in range(len(raw.programs))
                ]
            for index, p in enumerate(programs):
                result = subprocess.run(
                    [str(rust), path.suffix[1:].lower(), str(path), str(index), str(config_path)],
                    capture_output=True,
                    encoding="utf-8",
                    check=False,
                )
                expected = json.loads(result.stdout)
                try:
                    c = gxwlib.compile_program(p, profile="fx3g")
                except gxwlib.SimulationError as e:
                    assert result.returncode == 3
                    assert {
                        "error": str(e),
                        "code": e.code,
                        "instruction": e.instruction,
                    } == expected
                    records.append(
                        {"file": str(path), "program": index, "compiled": False, "code": e.code}
                    )
                    continue
                assert result.returncode == 0, result.stderr
                verify(c, expected["compiled"])
                s = gxwlib.Simulator(
                    c, scan_period_ns=config["period_ns"], initial_state=config["initial_state"]
                )
                for inputs, snapshot in zip(config["inputs"], expected["snapshots"], strict=True):
                    s.set_inputs(inputs)
                    r = s.step(trace_instructions=True)
                    verify(r, snapshot)
                    for event in r.instruction_trace:
                        assert c.program.read_source(event.source) == p.read_source(
                            p.instructions[event.instruction].source
                        )
                trace = s.run(config["run_scans"], watch=config["watch"])
                verify(trace, expected["trace"])
                verify(trace.final_snapshot, expected["trace"]["final_snapshot"])
                s.reset(initial_state=config["initial_state"])
                verify(s.snapshot(), expected["reset"])
                records.append(
                    {
                        "file": str(path),
                        "program": index,
                        "compiled": True,
                        "step_scans": 3,
                        "run_scans": 5,
                    }
                )
    assert files
    return {
        "simulation_binding_parity": "passed",
        "files": len(files),
        "programs": len(records),
        "compiled": sum(r["compiled"] for r in records),
        "rejected": sum(not r["compiled"] for r in records),
        "records": records,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path("tests/fixtures"))
    parser.add_argument("--rust-simulation", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = run(args.root.resolve(), args.rust_simulation.resolve())
    if args.output:
        args.output.write_text(
            json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )
    print(json.dumps({k: v for k, v in result.items() if k != "records"}))


if __name__ == "__main__":
    main()
