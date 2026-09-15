import json
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from hashlib import sha256
from pathlib import Path

import pytest

import gxwlib

FIXTURE = Path(__file__).resolve().parents[1] / "fixtures" / "raw.gxw"
BODY = bytes.fromhex("03 ee 03 04 9c 01 04 03 34 03")


def test_raw_views_ownership_source_mapping_and_unknown_semantics(tmp_path):
    path = tmp_path / "日本語.gxw"
    data = FIXTURE.read_bytes()
    path.write_bytes(data)
    project = gxwlib.load_project(path)
    (program,) = project.programs
    assert project.framing_complete
    assert project.semantic_status == "not_decoded"
    assert project.cpu_model is None and project.execution_order is None
    assert project.index.sha256 == sha256(data).hexdigest()
    for source in project.sources:
        assert sha256(project.read_source(source.source)).hexdigest() == source.sha256
    assert program.logical_name == "MAIN.プログラム.pou"
    assert program.profile == "simple-le-observed-v1"
    assert program.framing_status == "complete"
    assert [t.source.offset for t in program.tokens] == [79, 82, 86]
    assert b"".join(t.data for t in program.tokens) == BODY
    assert project.read_source(program.token_region) == BODY
    assert project.read_source(program.trailer) == bytes(20)
    assert "MAIN.プログラム.pou" in project.read_source(program.metadata_source).decode()
    other = gxwlib.loads(data)
    assert json.loads(project.to_json()) == json.loads(other.to_json())
    with pytest.raises(ValueError, match="different project"):
        other.read_source(program.source)
    token = program.tokens[0]
    assert token.data == bytes.fromhex("03 ee 03")
    assert project.read_source(token.source) == token.data
    assert path.read_bytes() == data
    path.unlink()
    del data, project
    assert token.data_hex == "03 ee 03"
    assert program.trailer.length == 20
    with pytest.raises(AttributeError):
        token.ordinal = 3


def test_partial_frame_retains_prefix_and_remainder():
    data = bytearray(FIXTURE.read_bytes())
    start = data.index(BODY)
    data[start + 3] = 0
    project = gxwlib.loads(bytes(data))
    (program,) = project.programs
    assert not project.framing_complete
    assert program.framing_status == "partial"
    assert len(program.tokens) == 1
    assert program.diagnostics[0].code == "GXW_TOKEN_FRAME_INVALID"
    remainder = program.opaque_regions[1]
    assert remainder.source.offset == 82
    assert project.read_source(remainder.source) == bytes(data[start + 3 : start + len(BODY)])


def test_unknown_profile_limits_and_bad_types():
    project = gxwlib.load_project(FIXTURE.with_name("minimal.gxw"))
    assert project.programs[0].framing_status == "unsupported"
    assert not project.programs[0].tokens
    assert project.programs[0].opaque_regions
    with pytest.raises(gxwlib.ResourceLimitError, match="tokens"):
        gxwlib.load_project(FIXTURE, limits=gxwlib.ReadLimits(max_tokens=2))
    with pytest.raises(gxwlib.ResourceLimitError, match="input bytes"):
        gxwlib.loads(FIXTURE.read_bytes(), limits=gxwlib.ReadLimits(max_file_bytes=1))
    with pytest.raises(TypeError):
        gxwlib.load_project(b"bytes path")
    with pytest.raises(TypeError):
        gxwlib.loads(bytearray())
    with pytest.raises(gxwlib.FormatError):
        gxwlib.loads(b"not CFB")


def test_raw_parallel_loads():
    with ThreadPoolExecutor(max_workers=4) as pool:
        result = list(pool.map(lambda _: gxwlib.load_project(FIXTURE).to_json(), range(8)))
    assert len(set(result)) == 1


def test_parse_cli_complete_and_unsupported_exit_status():
    cmd = [sys.executable, "-m", "gxwlib", "parse"]
    complete = subprocess.run(
        [*cmd, str(FIXTURE), "--json"], capture_output=True, encoding="utf-8", check=True
    )
    assert json.loads(complete.stdout)["framing_complete"]
    text = subprocess.run([*cmd, str(FIXTURE)], capture_output=True, encoding="utf-8", check=True)
    assert "03 ee 03" in text.stdout and "not_decoded" in text.stdout
    unknown = subprocess.run(
        [*cmd, str(FIXTURE.with_name("minimal.gxw")), "--json"],
        capture_output=True,
        encoding="utf-8",
        check=False,
    )
    assert unknown.returncode == 3
    assert json.loads(unknown.stdout)["programs"][0]["framing_status"] == "unsupported"
