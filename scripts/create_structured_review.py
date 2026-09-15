"""Create native block geometry review pages and independently read source previews."""

import argparse
import json
from pathlib import Path

import gxwlib as gxw


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--gxw", type=Path, action="append")
    args = p.parse_args()
    files = args.gxw or [Path(__file__).resolve().parents[1] / "tests/fixtures/structured.gxw"]
    args.output.mkdir(parents=True, exist_ok=True)
    cases = []
    for i, path in enumerate(files):
        raw = gxw.load_project(path)
        program = gxw.decode_structured(raw, 0)
        doc = gxw.render_structured(program)
        name = f"structured-{i}.html"
        (args.output / name).write_text(doc.to_html(), encoding="utf-8")
        elements = json.loads(doc.to_json())["elements"]
        for e, view in zip(elements, doc.elements, strict=True):
            e["bytes_preview"] = raw.read_source(view.source)[:64].hex(" ")
        cases.append({"file": name, "sha256": raw.index.sha256, "elements": elements})
    (args.output / "structured-cases.json").write_text(
        json.dumps(cases, indent=2), encoding="utf-8"
    )


if __name__ == "__main__":
    main()
