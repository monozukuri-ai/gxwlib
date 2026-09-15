"""Compare Rust, typed Python and CLI JSON on the self-authored structured fixture."""

import argparse
import json
import subprocess
import sys
from pathlib import Path

import gxwlib as gxw


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rust-structured", type=Path, required=True)
    parser.add_argument(
        "--fixture",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "tests/fixtures/structured.gxw",
    )
    args = parser.parse_args()
    project = gxw.load_project(args.fixture)
    for mode, index, flag, command, decoded in [
        ("pou", "0", "--program-index", "structured", gxw.decode_structured(project, 0)),
        (
            "declarations",
            "1",
            "--logical-index",
            "declarations",
            gxw.decode_declarations(project, 1),
        ),
    ]:
        rust = subprocess.check_output(
            [str(args.rust_structured.resolve()), mode, str(args.fixture), index], encoding="utf-8"
        )
        cli = subprocess.check_output(
            [sys.executable, "-m", "gxwlib", command, str(args.fixture), flag, index, "--json"],
            encoding="utf-8",
        )
        assert json.loads(rust) == json.loads(cli) == json.loads(decoded.to_json())
    print(json.dumps({"structured_parity": "passed", "cases": 2}))


if __name__ == "__main__":
    main()
