import gc
import json
import subprocess
import sys
from pathlib import Path

import pytest

import gxwlib as gxw

FIXTURE = Path(__file__).parents[1] / "fixtures" / "structured.gxw"


def test_cpu_spelling_profiles_do_not_imply_execution_support():
    assert gxw.parse_device_address("X10", family="fx").address == 8
    for family in ["q", "l"]:
        address = gxw.parse_device_address("x01f", family=family)
        assert (address.address, address.radix, address.canonical) == (31, 16, "X1F")
        assert address.family == family and address.device == "X"
        assert address.range_status == "not_checked"
        assert gxw.parse_device_address("T200", family=family).address == 200
    for text, family in [("X8", "fx"), ("DFF", "q"), ("X+1", "l"), ("X1Z0", "q")]:
        with pytest.raises(gxw.FormatError):
            gxw.parse_device_address(text, family=family)
    with pytest.raises(gxw.UnsupportedFeatureError):
        gxw.parse_device_address("局所ラベル", family="q")


def test_typed_views_source_lifetime_and_ir_dispatch():
    raw = gxw.load_project(FIXTURE)
    p = gxw.decode_ir(raw, 0, device_profile="fx")
    assert isinstance(p, gxw.StructuredProgram)
    assert p.structure_complete and p.semantic_status == "not_evaluated"
    block = p.blocks[0]
    node = block.nodes[1]
    port = node.ports[0]
    assert block.nets[1].ports == [(0, 1), (1, 0)]
    assert block.nets[1].wires == [0]
    assert port.position == (6, 2) and port.local == (0, 1)
    assert len(raw.read_source(port.source)) == 16
    with pytest.raises(ValueError, match="different project"):
        gxw.load_project(FIXTURE).read_source(port.source)
    with pytest.raises(AttributeError):
        node.symbol = "changed"
    with pytest.raises(TypeError):
        gxw.compile_program(p, profile="fx3g")
    del p, block, raw
    gc.collect()
    assert node.symbol == "積算器" and node.type_name == "MY_FB"
    assert port.source.length == 16
    simple = gxw.load_project(FIXTURE.with_name("raw.gxw"))
    assert isinstance(gxw.decode_ir(simple, 0, device_profile="fx"), gxw.InstructionProgram)


def test_declarations_and_geometry_json_views():
    raw = gxw.load_project(FIXTURE)
    p = gxw.decode_structured(raw, 0)
    tree = json.loads(p.to_json())
    assert tree["blocks"][0]["nodes"][1]["symbol"] == p.blocks[0].nodes[1].symbol
    labels = gxw.decode_declarations(raw, 1)
    row = labels.rows[0]
    assert labels.scope == "local" and labels.owner_name == "MAIN"
    assert row.name == "積算器" and row.type_code == 15 and row.type_reference == "MY_FB"
    assert row.initial_value == "" and row.comment == "<script>& comment"
    assert len(raw.read_source(row.source)) == row.source.length
    assert json.loads(labels.to_json())["rows"][0]["comment"] == row.comment


def test_render_escaping_partial_status_and_limits():
    raw = gxw.load_project(FIXTURE)
    p = gxw.decode_structured(raw, 0)
    doc = gxw.render_structured(p)
    assert len(doc.elements) == 14
    assert doc.svg == doc._repr_svg_()
    assert "X1&lt;&amp;" in doc.svg and "[opaque FB]" in doc.svg
    assert "\\u003c" in doc.to_html()
    for e in doc.elements:
        assert len(raw.read_source(e.source)) == e.source.length
    with pytest.raises(gxw.ResourceLimitError):
        gxw.render_structured(p, max_elements=1)
    with pytest.raises(gxw.ResourceLimitError):
        gxw.render_structured(p, max_output_bytes=100)
    for kwargs in [
        {"max_blocks": 0},
        {"max_records": 0},
        {"max_ports": 0},
        {"max_string_units": 0},
        {"max_connection_checks": 0},
    ]:
        with pytest.raises(gxw.ResourceLimitError):
            gxw.decode_structured(raw, 0, limits=gxw.StructuredLimits(**kwargs))
    with pytest.raises(gxw.ResourceLimitError):
        gxw.decode_declarations(raw, 1, limits=gxw.StructuredLimits(max_declarations=0))
    with pytest.raises(gxw.UnsupportedFeatureError):
        gxw.decode_structured(raw, 9)
    with pytest.raises(ValueError):
        gxw.decode_ir(raw, 0, device_profile="q")


def test_cli_parity_and_input_preservation(tmp_path):
    path = tmp_path / "日本語.gxw"
    data = FIXTURE.read_bytes()
    path.write_bytes(data)
    raw = gxw.load_project(path)
    for command, index_flag, index, expected in [
        ("structured", "--program-index", "0", gxw.decode_structured(raw, 0)),
        ("declarations", "--logical-index", "1", gxw.decode_declarations(raw, 1)),
    ]:
        result = subprocess.run(
            [sys.executable, "-m", "gxwlib", command, str(path), index_flag, index, "--json"],
            capture_output=True,
            text=True,
            check=True,
        )
        assert json.loads(result.stdout) == json.loads(expected.to_json())
    for format in ["svg", "html"]:
        output = tmp_path / f"view.{format}"
        subprocess.run(
            [
                sys.executable,
                "-m",
                "gxwlib",
                "render-structured",
                str(path),
                "--format",
                format,
                "--output",
                str(output),
            ],
            check=True,
        )
        assert "<svg " in output.read_text()
    result = subprocess.run(
        [sys.executable, "-m", "gxwlib", "render-structured", str(path), "--output", str(path)],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 1 and "output must differ" in result.stderr
    assert path.read_bytes() == data
