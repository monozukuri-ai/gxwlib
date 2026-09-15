import gc
import json
import subprocess
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

import pytest
from test_instructions import csv_data

import gxwlib

FIXTURES = Path(__file__).resolve().parents[1] / "fixtures"


def program(sequence):
    rows = []
    for n, item in enumerate(sequence.split(";")):
        op, *operands = item.split()
        rows.append((n, op, operands[0] if operands else ""))
        rows.extend(("", "", value) for value in operands[1:])
    return gxwlib.parse_csv_bytes(csv_data(rows), device_profile="fx")


def test_xref_external_writers_partial_accesses_and_owned_sources():
    p = program("LD X0;OUT M0;LD M0;SET Y0;LD X1;RST Y0;LD M1;OUT Y1;LD M2;OUT Y1;END")
    options = gxwlib.AnalysisOptions(external_writes=["m01"])
    assert options.external_writes == ["M1"]
    report = gxwlib.analyze(p, options=options)
    assert report.complete
    assert [(a.device.name, a.instruction, a.mode) for a in report.accesses] == [
        ("X0", 0, "read"),
        ("M0", 1, "write"),
        ("M0", 2, "read"),
        ("Y0", 3, "write"),
        ("X1", 4, "read"),
        ("Y0", 5, "write"),
        ("M1", 6, "read"),
        ("Y1", 7, "write"),
        ("M2", 8, "read"),
        ("Y1", 9, "write"),
    ]
    (double,) = [f for f in report.diagnostics if f.code == "GXW_MULTIPLE_WRITES"]
    assert double.related_instructions == [7, 9] and double.severity == "warning"
    (info,) = [f for f in report.diagnostics if f.code == "GXW_NO_KNOWN_LOCAL_WRITE"]
    assert info.instruction == 8 and info.severity == "info"
    access = report.accesses[2]
    data = p.read_source(access.source)
    other = program("LD X0;OUT Y0;END")
    with pytest.raises(ValueError, match="another"):
        other.read_source(access.source)
    del p
    gc.collect()
    assert report.program.read_source(access.source) == data
    with pytest.raises(AttributeError):
        access.instruction = 99
    partial = gxwlib.analyze(program("LD X0;OUT T0 D0;MOV D0 D1;LD label;OUT Y0;END"))
    assert not partial.complete
    assert [(a.operand, a.mode) for a in partial.accesses if a.instruction == 1] == [
        (0, "write"),
        (1, "read"),
    ]
    assert [a.mode for a in partial.accesses if a.instruction == 2] == ["unknown", "unknown"]
    assert any(f.code == "GXW_OPERAND_UNRESOLVED" for f in partial.diagnostics)


@pytest.mark.parametrize("name", ["or_blocks", "and_blocks", "saved_results", "cascade_blocks"])
def test_manual_graph_svg_metadata_and_notebook_adapter(name):
    p = gxwlib.load_csv(FIXTURES / "ladder" / f"{name}.csv", device_profile="fx")
    oracle = json.loads((FIXTURES / "ladder" / f"{name}.json").read_text())
    doc = gxwlib.render_ladder(p)
    assert doc.complete and doc.graph.complete
    assert doc._repr_svg_() == doc.svg
    assert doc.layout == "output_conditions_relaid"
    root = ET.fromstring(doc.svg)
    namespace = {"s": "http://www.w3.org/2000/svg"}
    assert json.loads(root.find("s:metadata", namespace).text) == json.loads(doc.to_json())
    coils = [e for e in doc.elements if e.kind == "coil"]
    assert [(e.instruction, e.label) for e in coils] == [
        (e["instruction"], e["device"]) for e in oracle["outputs"]
    ]
    for e in doc.elements:
        node = root.find(f".//s:g[@id='{e.id}']", namespace)
        assert int(node.attrib["data-instruction"]) == e.instruction
        assert int(node.attrib["data-offset"]) == e.source.offset
        assert doc.program.read_source(e.source) == p.read_source(
            p.instructions[e.instruction].source
        )
    assert "GX Works2" in doc.to_html()


def test_partial_graph_keeps_original_list_and_bounded_failures():
    p = program("LD X0;OUT Y0;MOV K0 D0;LD X1;OUT Y1;END")
    doc = gxwlib.render_ladder(p)
    assert not doc.complete and doc.graph.processed_instructions == 2
    assert [o.instruction for o in doc.graph.outputs] == [1]
    assert "PARTIAL" in doc.svg and '"MOV"' in doc.to_html()
    assert len(doc.program.instructions) == 6
    with pytest.raises(gxwlib.FormatError):
        gxwlib.AnalysisOptions(external_writes=["X8"])
    with pytest.raises(gxwlib.ResourceLimitError):
        gxwlib.analyze(p, options=gxwlib.AnalysisOptions(max_instructions=1))
    with pytest.raises(gxwlib.FormatError):
        gxwlib.build_ladder(p, options=gxwlib.AnalysisOptions(max_mps_depth=0))
    with pytest.raises(gxwlib.ResourceLimitError):
        gxwlib.render_ladder(p, max_elements=1)
    with pytest.raises(gxwlib.ResourceLimitError):
        gxwlib.render_ladder(p, max_output_bytes=10)
    for sequence, code in [
        ("LD X0;MPP;OUT Y0;END", "GXW_MPS_UNDERFLOW"),
        ("LD X0;ANB;OUT Y0;END", "GXW_BLOCK_UNDERFLOW"),
    ]:
        report = gxwlib.analyze(program(sequence))
        assert not report.complete and any(f.code == code for f in report.diagnostics)


def test_diff_sources_canonicalization_and_unknown_status():
    left = program("LD X010;OUT Y0;END")
    equal = gxwlib.diff_programs(left, program("ld X10;out Y0;end"))
    assert equal.complete and not equal.different and not equal.opaque_changed
    right = program("LD X10;AND M0;OUT Y1;END")
    diff = gxwlib.diff_programs(left, right)
    assert diff.complete and diff.different
    assert [h.kind for h in diff.hunks] == ["equal", "replace", "equal"]
    h = diff.hunks[1]
    assert h.left_count == 1 and h.right_count == 2
    assert left.read_source(h.left_sources[0]) == diff.left_program.read_source(h.left_sources[0])
    assert right.read_source(h.right_sources[0]) == diff.right_program.read_source(
        h.right_sources[0]
    )
    with pytest.raises(ValueError, match="another"):
        left.read_source(h.right_sources[0])
    del left, right
    gc.collect()
    assert diff.right_program.read_source(h.right_sources[1])
    unknown = program("MOV D0 D1;END")
    result = gxwlib.diff_programs(unknown, unknown)
    assert not result.complete and not result.different
    with pytest.raises(gxwlib.ResourceLimitError):
        gxwlib.diff_programs(program("LD X0;OUT Y0;END"), program("LD X1;OUT Y1;END"), max_cells=1)


def cli(*args):
    return subprocess.run(
        [sys.executable, "-m", "gxwlib", *map(str, args)],
        capture_output=True,
        encoding="utf-8",
        check=False,
    )


def test_analysis_render_diff_cli_exit_codes_and_input_preservation(tmp_path):
    path = tmp_path / "日本語.csv"
    data = csv_data([(0, "LD", "X0"), (1, "OUT", "Y0"), (2, "END", "")])
    path.write_bytes(data)
    common = [path, "--input-format", "csv", "--device-profile", "fx"]
    result = cli("analyze", *common, "--json")
    assert result.returncode == 0 and json.loads(result.stdout)["complete"]
    assert cli("analyze", *common, "--external-write", "X8").returncode == 1
    assert cli("analyze", *common, "--program", "1").returncode == 1
    assert cli("analyze", path).returncode == 2
    assert cli("render", *common, "--max-elements", "-1").returncode == 1
    output = tmp_path / "回路.html"
    assert cli("render", *common, "--format", "html", "--output", output).returncode == 0
    assert "<svg " in output.read_text(encoding="utf-8")
    assert cli("render", *common, "--output", output).returncode == 1
    assert cli("render", *common, "--output", output, "--force").returncode == 0
    assert output.read_text().startswith("<svg ")
    assert cli("render", *common, "--output", path, "--force").returncode == 1
    assert path.read_bytes() == data
    assert cli("diff", *common, path, "--json").returncode == 0
    other = FIXTURES / "ladder" / "or_blocks.csv"
    assert cli("diff", *common, other).returncode == 4
    partial = FIXTURES / "third_party" / "ladder_converter" / "MAIN.csv"
    assert cli("diff", *common, partial).returncode == 3
    assert (
        cli("analyze", partial, "--input-format", "csv", "--device-profile", "fx").returncode == 3
    )
    assert (
        cli(
            "render", partial, "--input-format", "csv", "--device-profile", "fx", "--format", "svg"
        ).returncode
        == 3
    )
    assert (
        cli("render", FIXTURES / "raw.gxw", "--device-profile", "fx", "--format", "svg").returncode
        == 3
    )
    assert cli("render", tmp_path / "missing.gxw", "--device-profile", "fx").returncode == 1
