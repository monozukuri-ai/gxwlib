"""Compare every instruction attribute and original source against Rust JSON."""

import argparse
import json
import subprocess
from pathlib import Path

import gxwlib


def span(s):
    return None if s is None else {"source_id": s.source_id, "offset": s.offset, "length": s.length}


def operand_value(v):
    if v.kind == "device":
        return {"kind": v.kind, "device": v.device, "address": v.address, "radix": v.radix}
    if v.kind == "constant":
        return {"kind": v.kind, "value": v.value, "radix": v.radix, "bit_width": v.bit_width}
    return {"kind": "unknown"}


def attributes(p):
    for ins in p.instructions:
        assert len(p.read_source(ins.source)) == ins.source.length
        for o in ins.operands:
            assert len(p.read_source(o.source)) == o.source.length
    for row in p.csv_rows:
        for text, source in zip(row.fields, row.field_sources, strict=True):
            assert p.read_source(source).decode("utf-16-le") == '"' + text.replace('"', '""') + '"'
    return {
        "schema_version": p.schema_version,
        "origin": p.origin,
        "source_sha256": p.source_sha256,
        "device_profile": p.device_profile,
        "logical_name": p.logical_name,
        "complete": p.complete,
        "instructions": [
            {
                "ordinal": i.ordinal,
                "step": i.step,
                "opcode": i.opcode,
                "mnemonic": i.mnemonic,
                "original_mnemonic": i.original_mnemonic,
                "source": span(i.source),
                "supported": i.supported,
                "operands": [
                    {
                        "value": operand_value(o.value),
                        "source": span(o.source),
                        "encoding_width_bits": o.encoding_width_bits,
                        "original_text": o.original_text,
                    }
                    for o in i.operands
                ],
            }
            for i in p.instructions
        ],
        "diagnostics": [
            {"code": d.code, "message": d.message, "source": span(d.source)} for d in p.diagnostics
        ],
        "opaque_regions": [span(s) for s in p.opaque_regions],
        "csv_rows": [
            {
                "number": r.number,
                "fields": r.fields,
                "source": span(r.source),
                "field_sources": [span(s) for s in r.field_sources],
            }
            for r in p.csv_rows
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rust-instructions", required=True, type=Path)
    parser.add_argument("--root", default=Path("tests/fixtures"), type=Path)
    args = parser.parse_args()
    count = 0
    for path in sorted(args.root.rglob("*")):
        if path.suffix.lower() == ".csv":
            actual = gxwlib.load_csv(path, device_profile="fx")
            expected = json.loads(
                subprocess.check_output(
                    [str(args.rust_instructions), "csv", str(path)], encoding="utf-8"
                )
            )
            assert attributes(actual) == json.loads(actual.to_json()) == expected
            count += 1
        elif path.suffix.lower() == ".gxw":
            raw = gxwlib.load_project(path)
            actual = [
                gxwlib.decode_program(raw, i, device_profile="fx") for i in range(len(raw.programs))
            ]
            expected = json.loads(
                subprocess.check_output(
                    [str(args.rust_instructions), "gxw", str(path)], encoding="utf-8"
                )
            )
            assert (
                [attributes(p) for p in actual]
                == [json.loads(p.to_json()) for p in actual]
                == expected
            )
            count += 1
    assert count > 0
    print(json.dumps({"instruction_binding_parity": "passed", "files": count}))


if __name__ == "__main__":
    main()
