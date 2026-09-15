"""Check wheel/sdist contents, licenses, RECORD, versions and Git exclusions."""

import argparse
import base64
import csv
import io
import json
import subprocess
import tarfile
import tomllib
import zipfile
from email.parser import BytesParser
from hashlib import sha256
from pathlib import Path, PurePosixPath

from check_public_docs import check as check_public_docs


def check_git_exclusions(root):
    if not (root / ".git").exists():
        return "not applicable (source archive)"
    tracked = subprocess.check_output(
        ["git", "ls-files", "--cached", "--ignored", "--exclude-standard", "-z"], cwd=root
    )
    assert not tracked, "Git tracks ignored paths: " + repr(tracked.split(b"\0")[:-1])
    return "passed"


def check_license_metadata(metadata, notices):
    assert metadata["License-Expression"] == "MIT"
    assert set(metadata.get_all("License-File", [])) == {"LICENSE"} | {
        "LICENSES/" + name for name in notices
    }


def safe_name(name):
    path = PurePosixPath(name.replace("\\", "/"))
    assert not path.is_absolute() and ".." not in path.parts, name
    assert not set(path.parts) & {".internal", "data", ".git", ".venv", "target", "__pycache__"}, (
        name
    )
    assert not name.endswith((".pdf", ".pyc")), name
    assert path.name not in {"check_corpus.py", "check_cold_corpus.py"}, name
    assert not path.as_posix().endswith("docs/development.md"), name
    return path.as_posix()


def check_wheel(path, version, notices, root):
    with zipfile.ZipFile(path) as wheel:
        names = wheel.namelist()
        assert len(names) == len(set(names)), "duplicate wheel member"
        for name in names:
            safe_name(name)
        assert "cp313-abi3-" in path.name, path
        for name in [
            "__init__.py",
            "api.py",
            "cli.py",
            "analysis_cli.py",
            "simulation_cli.py",
            "structured_cli.py",
            "__main__.py",
            "_core.pyi",
            "py.typed",
        ]:
            assert f"gxwlib/{name}" in names, name
            assert wheel.read(f"gxwlib/{name}") == (root / "src" / "gxwlib" / name).read_bytes(), (
                name
            )
        extensions = [
            n for n in names if n.startswith("gxwlib/_core") and n.endswith((".so", ".pyd"))
        ]
        assert len(extensions) == 1, extensions
        (metadata_name,) = [n for n in names if n.endswith(".dist-info/METADATA")]
        metadata = BytesParser().parsebytes(wheel.read(metadata_name))
        assert metadata["Name"] == "gxwlib"
        assert metadata["Version"] == version
        assert metadata["Requires-Python"] == ">=3.13"
        check_license_metadata(metadata, notices)
        (license_name,) = [n for n in names if n.endswith(".dist-info/licenses/LICENSE")]
        assert wheel.read(license_name) == (root / "LICENSE").read_bytes()
        assert not metadata.get_all("Requires-Dist"), "unexpected runtime dependency"
        assert (
            metadata.get_payload().strip()
            == (root / "README.md").read_text(encoding="utf-8").strip()
        )
        for name, data in notices.items():
            (member,) = [n for n in names if n.endswith("/licenses/LICENSES/" + name)]
            assert wheel.read(member) == data, name
        (record_name,) = [n for n in names if n.endswith(".dist-info/RECORD")]
        records = list(csv.reader(io.StringIO(wheel.read(record_name).decode())))
        recorded = set()
        for raw_name, digest, size in records:
            # Some Windows builders use backslashes only in RECORD paths.
            name = safe_name(raw_name)
            assert name not in recorded and name in names, name
            recorded.add(name)
            if name == record_name:
                assert digest == size == ""
                continue
            data = wheel.read(name)
            expected = base64.urlsafe_b64encode(sha256(data).digest()).rstrip(b"=").decode()
            assert digest == "sha256=" + expected and int(size) == len(data), name
        assert recorded == set(names), "unrecorded wheel members"


def check_sdist(path, version, notices, root):
    with tarfile.open(path) as archive:
        members = archive.getmembers()
        names = [m.name for m in members]
        assert len(names) == len(set(names)), "duplicate sdist member"
        assert {PurePosixPath(n).parts[0] for n in names} == {f"gxwlib-{version}"}
        for member in members:
            safe_name(member.name)
            assert member.isfile() or member.isdir(), member.name
        files = {
            PurePosixPath(m.name).relative_to(f"gxwlib-{version}").as_posix(): archive.extractfile(
                m
            ).read()
            for m in members
            if m.isfile()
        }
        required = [
            "LICENSE",
            "README.md",
            "docs/README.md",
            "scripts/README.md",
            "scripts/check_public_docs.py",
            "Cargo.toml",
            "Cargo.lock",
            "uv.lock",
            "pyproject.toml",
            "rust-toolchain.toml",
            "crates/gxw-core/src/lib.rs",
            "crates/gxw-python/src/lib.rs",
            "src/gxwlib/_core.pyi",
            "tests/fixtures/minimal.gxw",
            "tests/python/test_inspect.py",
            "scripts/smoke_installed.py",
            "scripts/check_sdist_build.py",
        ]
        for name in required:
            assert name in files, name
        assert files["README.md"] == (root / "README.md").read_bytes()
        assert files["LICENSE"] == (root / "LICENSE").read_bytes()
        check_license_metadata(BytesParser().parsebytes(files["PKG-INFO"]), notices)
        project = tomllib.loads(files["pyproject.toml"].decode())["project"]
        assert project["license"] == "MIT"
        # Cargo.toml/pyproject.toml are normalized by maturin; compare source and
        # verification payloads byte-for-byte and resolve manifests separately.
        for folder in ["src", "crates", "tests", "scripts", "docs"]:
            for source in (root / folder).rglob("*"):
                if source.is_file() and (
                    source.suffix
                    in {
                        ".rs",
                        ".py",
                        ".pyi",
                        ".md",
                        ".gxw",
                        ".csv",
                        ".json",
                        ".txt",
                        ".html",
                        ".mjs",
                    }
                    or source.name in {"py.typed", "LICENSE"}
                ):
                    name = source.relative_to(root).as_posix()
                    assert files.get(name) == source.read_bytes(), name
        for name in ["Cargo.lock", "uv.lock", "rust-toolchain.toml"]:
            assert files[name] == (root / name).read_bytes(), name
        for name, data in notices.items():
            assert files["LICENSES/" + name] == data
        assert not any(n.endswith((".so", ".pyd", ".whl")) for n in files)
        workspace = tomllib.loads(files["Cargo.toml"].decode())["workspace"]
        assert workspace["package"]["version"] == version
        assert workspace["package"]["license"] == "MIT"
        assert workspace["dependencies"]["gxw-core"]["version"] == "=" + version
        assert workspace["dependencies"]["gxw-core"]["path"] == "crates/gxw-core"
        for name in ["crates/gxw-core/Cargo.toml", "crates/gxw-python/Cargo.toml"]:
            manifest = tomllib.loads(files[name].decode())
            declared = manifest["package"]["version"]
            resolved = (
                workspace["package"]["version"] if declared == {"workspace": True} else declared
            )
            assert resolved == version, name
            license_value = manifest["package"]["license"]
            resolved_license = (
                workspace["package"]["license"]
                if license_value == {"workspace": True}
                else license_value
            )
            assert resolved_license == "MIT", name


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dist", type=Path, default=Path("dist"))
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    documentation = check_public_docs(root)
    git_exclusions = check_git_exclusions(root)
    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    version = manifest["workspace"]["package"]["version"]
    assert manifest["workspace"]["dependencies"]["gxw-core"]["version"] == "=" + version
    assert manifest["workspace"]["package"]["license"] == "MIT"
    project = tomllib.loads((root / "pyproject.toml").read_text())["project"]
    assert project["license"] == "MIT"
    assert project["license-files"] == ["LICENSE", "LICENSES/*"]
    for crate in ["gxw-core", "gxw-python"]:
        cargo = tomllib.loads((root / "crates" / crate / "Cargo.toml").read_text())
        assert cargo["package"]["version"] == {"workspace": True}
        assert cargo["package"]["license"] == {"workspace": True}
        assert (root / "crates" / crate / "LICENSE").read_bytes() == (root / "LICENSE").read_bytes()
    notices = {p.name: p.read_bytes() for p in (root / "LICENSES").iterdir() if p.is_file()}
    assert notices, "missing dependency notices"
    locked = tomllib.loads((root / "Cargo.lock").read_text())["package"]
    registry = {(p["name"], p["version"]) for p in locked if p.get("source")}
    inventory = json.loads(notices["inventory.json"])
    assert {(p["name"], p["version"]) for p in inventory} == registry
    assert {p["notice"] for p in inventory} | {"inventory.json", "gxworks-agent.txt"} == set(
        notices
    )
    wheels = sorted(args.dist.glob("*.whl"))
    sdists = sorted(args.dist.glob("*.tar.gz"))
    assert wheels and len(sdists) == 1, "expected wheels and one sdist"
    for path in wheels:
        check_wheel(path, version, notices, root)
    check_sdist(sdists[0], version, notices, root)
    print(
        json.dumps(
            {
                "version": version,
                "wheels": len(wheels),
                "sdists": 1,
                "contents_and_record": "passed",
                "license": "MIT",
                "git_exclusions": git_exclusions,
                "documentation": documentation,
            }
        )
    )


if __name__ == "__main__":
    main()
