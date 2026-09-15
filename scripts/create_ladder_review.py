"""Generate reproducible browser review cases; all outputs stay in --output."""

import argparse
import csv
import io
import json
from pathlib import Path

import gxwlib


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--gxw", type=Path, action="append", default=[])
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    root = Path(__file__).resolve().parents[1]
    cases = []
    inputs = [
        (path.stem, gxwlib.load_csv(path, device_profile="fx"))
        for path in sorted((root / "tests/fixtures/ladder").glob("*.csv"))
    ]
    for path in args.gxw:
        raw = gxwlib.load_project(path)
        inputs.extend(
            (f"{path.stem}-{i}", gxwlib.decode_program(raw, i, device_profile="fx"))
            for i in range(len(raw.programs))
        )
    text = io.StringIO(newline="")
    writer = csv.writer(text, delimiter="\t", quoting=csv.QUOTE_ALL, lineterminator="\r\n")
    attack = '{{DATA}}{{SVG}}</script><script>window.attacked=1</script>&"日本語'
    writer.writerows(
        [
            [attack],
            ["PC情報:", "FXCPU FX3G"],
            [
                "ステップ番号",
                "行間ステートメント",
                "命令",
                "I/O(デバイス)",
                "空欄",
                "PIステートメント",
                "ノート",
            ],
            ["0", "", "LDI", "X0", "", "", ""],
            ["1", "", "SET", "Y0", "", "", ""],
            ["2", "", "MOV", attack, "", "", ""],
            ["3", "", "END", "", "", "", ""],
        ]
    )
    inputs.append(
        (
            "partial_escaped",
            gxwlib.parse_csv_bytes(
                b"\xff\xfe" + text.getvalue().encode("utf-16-le"), device_profile="fx"
            ),
        )
    )
    for name, program in inputs:
        doc = gxwlib.render_ladder(program)
        path = args.output / f"{name}.html"
        path.write_text(doc.to_html(), encoding="utf-8")
        (args.output / f"{name}.svg").write_text(doc.svg, encoding="utf-8")
        cases.append(
            {
                "file": path.name,
                "document": json.loads(doc.to_json()),
                "program": json.loads(program.to_json()),
            }
        )
    (args.output / "cases.json").write_text(
        json.dumps(cases, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    print(json.dumps({"review_cases": len(cases), "output": str(args.output)}))


if __name__ == "__main__":
    main()
