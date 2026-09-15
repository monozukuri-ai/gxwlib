import ast
import json
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from hashlib import sha256
from pathlib import Path

import pytest

import gxwlib
from gxwlib import _core

FIXTURE = Path(__file__).resolve().parents[1] / "fixtures" / "minimal.gxw"


def test_inspect_and_owned_views(tmp_path):
    path = tmp_path / "日本語 project.gxw"
    original = FIXTURE.read_bytes()
    path.write_bytes(original)
    project = gxwlib.inspect(path)
    assert project.size == len(original)
    assert project.sha256 == sha256(original).hexdigest()
    assert [c.path for c in project.containers] == [[], ["_hdb"]]
    assert [len(c.streams) for c in project.containers] == [5, 2]
    stream = project.containers[1].streams[0]
    logical = project.logical_objects[0]
    assert logical.logical_name == "MAIN.プログラム.pou"
    assert logical.location.container == ["_hdb"]
    assert logical.location.path == ["7"]
    assert stream.size == len(b"opaque synthetic program; not PLC instructions")
    assert stream.sha256 == sha256(b"opaque synthetic program; not PLC instructions").hexdigest()
    assert project.logical_objects[1].location.container == []
    assert project.diagnostics[0].code == "GXW_INSPECT_ONLY"
    assert json.loads(project.to_json()) == json.loads(gxwlib.inspect_bytes(original).to_json())
    assert path.read_bytes() == original
    path.unlink()
    del project
    assert logical.id == "7"
    assert stream.path == ["7"]
    copy = logical.metadata.fields
    copy["iID"] = "changed"
    assert logical.metadata.fields["iID"] == "7"
    with pytest.raises(AttributeError):
        logical.id = "changed"


def test_limits_and_exceptions(tmp_path):
    limits = gxwlib.ReadLimits(max_file_bytes=1)
    assert limits.max_file_bytes == 1
    with pytest.raises(gxwlib.ResourceLimitError, match="input bytes"):
        gxwlib.inspect(FIXTURE, limits=limits)
    with pytest.raises(gxwlib.ResourceLimitError):
        gxwlib.inspect_bytes(FIXTURE.read_bytes(), limits=limits)
    with pytest.raises(gxwlib.FormatError):
        gxwlib.inspect_bytes(b"not CFB")
    with pytest.raises(FileNotFoundError):
        gxwlib.inspect(tmp_path / "missing.gxw")
    with pytest.raises(TypeError):
        gxwlib.inspect(b"bytes path")
    with pytest.raises(TypeError):
        gxwlib.inspect_bytes(bytearray(b"bad"))
    with pytest.raises(OverflowError):
        gxwlib.ReadLimits(max_file_bytes=-1)
    assert issubclass(gxwlib.FormatError, gxwlib.GxwError)


def test_parallel_calls_are_independent():
    with ThreadPoolExecutor(max_workers=4) as pool:
        results = list(pool.map(lambda _: gxwlib.inspect(FIXTURE).to_json(), range(12)))
    assert len(set(results)) == 1


def test_cli_json_text_and_failure(tmp_path):
    for executable in [[sys.executable, "-m", "gxwlib"]]:
        result = subprocess.run(
            [*executable, "inspect", str(FIXTURE), "--json"],
            capture_output=True,
            text=True,
            encoding="utf-8",
            check=True,
        )
        assert json.loads(result.stdout) == json.loads(gxwlib.inspect(FIXTURE).to_json())
        text = subprocess.run(
            [*executable, "inspect", str(FIXTURE)],
            capture_output=True,
            text=True,
            encoding="utf-8",
            check=True,
        )
        assert "MAIN.プログラム.pou" in text.stdout
        failed = subprocess.run(
            [*executable, "inspect", str(tmp_path / "missing")],
            capture_output=True,
            text=True,
            encoding="utf-8",
            check=False,
        )
        assert failed.returncode == 1
        assert "gxw:" in failed.stderr
        assert "Traceback" not in failed.stderr


def test_stub_matches_public_extension_members():
    stub = Path(gxwlib.__file__).with_name("_core.pyi")
    tree = ast.parse(stub.read_text())
    for node in tree.body:
        if isinstance(node, (ast.ClassDef, ast.FunctionDef)):
            assert hasattr(_core, node.name), node.name
        if isinstance(node, ast.ClassDef):
            runtime = getattr(_core, node.name)
            expected = {m.name for m in node.body if isinstance(m, ast.FunctionDef)}
            for name in expected:
                assert hasattr(runtime, name), f"{node.name}.{name}"
            if node.name not in {
                "GxwError",
                "FormatError",
                "ResourceLimitError",
                "UnsupportedFeatureError",
            }:
                actual = {name for name in vars(runtime) if not name.startswith("_")}
                assert actual == {name for name in expected if not name.startswith("_")}
    declared = {n.name for n in tree.body if isinstance(n, (ast.ClassDef, ast.FunctionDef))}
    actual = {n for n in vars(_core) if not n.startswith("_")}
    assert actual == declared
