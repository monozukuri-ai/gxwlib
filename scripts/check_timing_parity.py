"""Verify typed Python timing states, independent expectations and Rust parity."""

import argparse
import json
import subprocess
import tempfile
from pathlib import Path

from check_simulation_parity import attributes, verify

import gxwlib


def run(root, rust):
    cases = json.loads((root / "scenarios.json").read_text())["cases"]
    records = []
    with tempfile.TemporaryDirectory(prefix="gxw-timing-parity-") as temporary:
        config_path = Path(temporary) / "config.json"
        for case in cases:
            source = root / case["file"]
            p = gxwlib.load_csv(source, device_profile="fx")
            c = gxwlib.compile_program(p, profile="fx3g")
            watch = [d.name for d in c.devices]
            config = {
                "period_ns": case["period_ns"],
                "phase_ns": case["phase_ns"],
                "initial_state": case["initial"],
                "inputs": [],
                "actions": case["actions"],
                "run_scans": 5,
                "watch": watch,
            }
            config_path.write_text(json.dumps(config))
            result = subprocess.run(
                [str(rust), "csv", str(source), "0", str(config_path)],
                capture_output=True,
                text=True,
                check=True,
            )
            expected = json.loads(result.stdout)
            verify(c, expected["compiled"])
            s = gxwlib.Simulator(
                c,
                scan_period_ns=case["period_ns"],
                timer_phase_ns=case["phase_ns"],
                initial_state=case["initial"],
            )
            for action, rust_snapshot in zip(
                case["actions"], expected["action_snapshots"], strict=True
            ):
                match action["op"]:
                    case "step":
                        s.set_inputs(action["inputs"])
                        snap = s.step(trace_instructions=True)
                    case "period":
                        s.set_scan_period_ns(action["period_ns"])
                        snap = s.snapshot()
                    case "advance_stopped":
                        s.advance_stopped(action["elapsed_ns"])
                        snap = s.snapshot()
                    case op:
                        getattr(s, op)()
                        snap = s.snapshot()
                verify(snap, rust_snapshot)
                assert attributes(snap, action["expected"]) == action["expected"]
                for event in snap.instruction_trace:
                    assert p.read_source(event.source) == p.read_source(
                        p.instructions[event.instruction].source
                    )
            trace = s.run(5, watch=watch)
            verify(trace, expected["trace"])
            s.reset(initial_state=case["initial"])
            verify(s.snapshot(), expected["reset"])
            records.append(
                {
                    "case": case["name"],
                    "actions": len(case["actions"]),
                    "batched_scans": 5,
                    "status": "passed",
                }
            )
    return {
        "timing_parity": "passed",
        "cases": len(cases),
        "actions": sum(r["actions"] for r in records),
        "records": records,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path("tests/fixtures/timing"))
    parser.add_argument("--rust-simulation", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = run(args.root.resolve(), args.rust_simulation.resolve())
    if args.output:
        args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({k: v for k, v in result.items() if k != "records"}))


if __name__ == "__main__":
    main()
