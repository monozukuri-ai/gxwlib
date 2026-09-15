"""Compare Python attribute conversions with the Rust example's JSON output."""

import argparse
import json
import subprocess
from hashlib import sha256
from pathlib import Path

import gxwlib


def location(value):
    return None if value is None else {"container": value.container, "path": value.path}


def row(value):
    return {"fields": value.fields, "attributes": value.attributes, "ancestors": value.ancestors}


def attributes(index):
    return {
        "schema_version": index.schema_version,
        "size": index.size,
        "sha256": index.sha256,
        "containers": [
            {
                "path": c.path,
                "cfb_version": c.cfb_version,
                "streams": [
                    {"path": s.path, "size": s.size, "sha256": s.sha256} for s in c.streams
                ],
            }
            for c in index.containers
        ],
        "logical_objects": [
            {
                "id": o.id,
                "logical_name": o.logical_name,
                "scrap": o.scrap,
                "historical": o.historical,
                "metadata": row(o.metadata),
                "location": location(o.location),
            }
            for o in index.logical_objects
        ],
        "project_rows": [row(r) for r in index.project_rows],
        "diagnostics": [
            {
                "code": d.code,
                "severity": d.severity,
                "message": d.message,
                "location": location(d.location),
            }
            for d in index.diagnostics
        ],
    }


def check(path, rust_inspect):
    expected = json.loads(subprocess.check_output([str(rust_inspect), str(path)], encoding="utf-8"))
    index = gxwlib.inspect(path)
    assert attributes(index) == expected, f"binding attribute mismatch: {path}"
    assert json.loads(index.to_json()) == expected, f"JSON mismatch: {path}"
    return expected


def span(value):
    return (
        None
        if value is None
        else {"source_id": value.source_id, "offset": value.offset, "length": value.length}
    )


def parse_diagnostic(value):
    return {
        "code": value.code,
        "severity": value.severity,
        "message": value.message,
        "source": span(value.source),
    }


def raw_attributes(project):
    for source in project.sources:
        data = project.read_source(source.source)
        assert len(data) == source.size
        assert sha256(data).hexdigest() == source.sha256
    programs = []
    for program in project.programs:
        for token in program.tokens:
            assert project.read_source(token.source) == token.data
            assert token.data.hex(" ") == token.data_hex
        programs.append(
            {
                "logical_index": program.logical_index,
                "logical_name": program.logical_name,
                "metadata_source": span(program.metadata_source),
                "source": span(program.source),
                "profile": program.profile,
                "framing_status": program.framing_status,
                "token_region": span(program.token_region),
                "trailer": span(program.trailer),
                "tokens": [
                    {"ordinal": t.ordinal, "source": span(t.source), "data_hex": t.data_hex}
                    for t in program.tokens
                ],
                "opaque_regions": [
                    {"source": span(r.source), "reason": r.reason} for r in program.opaque_regions
                ],
                "diagnostics": [parse_diagnostic(d) for d in program.diagnostics],
            }
        )
    return {
        "schema_version": project.schema_version,
        "index": attributes(project.index),
        "sources": [
            {"id": s.id, "location": location(s.location), "size": s.size, "sha256": s.sha256}
            for s in project.sources
        ],
        "programs": programs,
        "diagnostics": [parse_diagnostic(d) for d in project.diagnostics],
        "framing_complete": project.framing_complete,
        "semantic_status": project.semantic_status,
        "cpu_model": project.cpu_model,
        "execution_order": project.execution_order,
    }


def check_raw(path, rust_parse):
    expected = json.loads(subprocess.check_output([str(rust_parse), str(path)], encoding="utf-8"))
    project = gxwlib.load_project(path)
    assert raw_attributes(project) == expected, f"raw attribute mismatch: {path}"
    assert json.loads(project.to_json()) == expected, f"raw JSON mismatch: {path}"
    return project


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path("tests/fixtures"))
    parser.add_argument("--rust-inspect", type=Path, required=True)
    parser.add_argument("--rust-parse", type=Path)
    args = parser.parse_args()
    files = sorted(p for p in args.root.rglob("*") if p.suffix.lower() == ".gxw")
    assert files, "no GXW files"
    for path in files:
        check(path, args.rust_inspect.resolve())
        if args.rust_parse:
            check_raw(path, args.rust_parse.resolve())
    print(json.dumps({"binding_parity_files": len(files)}))


if __name__ == "__main__":
    main()
