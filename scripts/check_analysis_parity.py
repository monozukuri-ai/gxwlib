"""Compare typed Python views and exact SVG/HTML against the Rust example."""

import argparse
import json
import subprocess
from itertools import pairwise
from pathlib import Path

import gxwlib


def attributes(value, shape):
    if isinstance(shape, dict):
        return {k: attributes(getattr(value, k), v) for k, v in shape.items()}
    if isinstance(shape, list):
        assert len(value) == len(shape)
        return [attributes(a, b) for a, b in zip(value, shape, strict=True)]
    return value


def verify(value, expected):
    assert attributes(value, expected) == json.loads(value.to_json()) == expected


def programs(path):
    if path.suffix.lower() == ".csv":
        return [gxwlib.load_csv(path, device_profile="fx")]
    raw = gxwlib.load_project(path)
    return [gxwlib.decode_program(raw, i, device_profile="fx") for i in range(len(raw.programs))]


def run(root, rust):
    count = complete = 0
    files = [p for p in sorted(root.rglob("*")) if p.suffix.lower() in {".csv", ".gxw"}]
    for path in files:
        expected = json.loads(
            subprocess.check_output(
                [str(rust), path.suffix[1:].lower(), str(path)], encoding="utf-8"
            )
        )
        actual = programs(path)
        assert len(actual) == len(expected)
        for p, e in zip(actual, expected, strict=True):
            report = gxwlib.analyze(p)
            verify(report, e["analysis"])
            verify(gxwlib.build_ladder(p), e["graph"])
            doc = gxwlib.render_ladder(p)
            verify(doc, e["document"])
            assert doc.svg == doc._repr_svg_() == e["svg"]
            assert doc.to_html() == e["html"]
            verify(gxwlib.diff_programs(p, p), e["diff_self"])
            for a in report.accesses:
                assert p.read_source(a.source) == report.program.read_source(a.source)
            for element in doc.elements:
                assert doc.program.read_source(element.source) == p.read_source(
                    p.instructions[element.instruction].source
                )
            complete += report.complete
            count += 1
    csvs = [p for p in files if p.suffix.lower() == ".csv"]
    pairs = 0
    for left, right in pairwise(csvs):
        expected = json.loads(
            subprocess.check_output(
                [str(rust), "csv", str(left), "csv", str(right)], encoding="utf-8"
            )
        )
        verify(gxwlib.diff_programs(programs(left)[0], programs(right)[0]), expected)
        pairs += 1
    assert files
    return {
        "analysis_binding_and_svg_html_parity": "passed",
        "files": len(files),
        "programs": count,
        "complete": complete,
        "partial": count - complete,
        "cross_program_diffs": pairs,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path("tests/fixtures"))
    parser.add_argument("--rust-analysis", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(run(args.root.resolve(), args.rust_analysis.resolve())))


if __name__ == "__main__":
    main()
