import hashlib
import sys
import zipfile
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))
from check_release import PLATFORMS, check_artifacts, release_version


@pytest.fixture
def release_root(tmp_path):
    (tmp_path / "Cargo.toml").write_text(
        '[workspace.package]\nversion = "0.1.0"\n'
        '[workspace.dependencies]\ngxw-core = { version = "=0.1.0" }\n'
    )
    (tmp_path / "Cargo.lock").write_text(
        '[[package]]\nname = "gxw-core"\nversion = "0.1.0"\n'
        '[[package]]\nname = "gxw-python"\nversion = "0.1.0"\n'
    )
    return tmp_path


def test_release_version_accepts_manual_build_and_matching_tag(release_root):
    assert release_version(release_root) == "0.1.0"
    assert release_version(release_root, "v0.1.0") == "0.1.0"


@pytest.mark.parametrize("tag", ["v0.2.0", "0.1.0", "v0.1.0-rc.1", "v0.1.0\nextra=1"])
def test_release_version_rejects_mismatched_tags(release_root, tag):
    with pytest.raises(AssertionError, match="must match"):
        release_version(release_root, tag)


@pytest.mark.parametrize("version", ["0.1.0-rc.1", "01.1.0", "0.1.0+local"])
def test_release_version_rejects_noncanonical_versions(release_root, version):
    manifest = release_root / "Cargo.toml"
    manifest.write_text(manifest.read_text().replace("0.1.0", version))
    with pytest.raises(AssertionError, match="MAJOR.MINOR.PATCH"):
        release_version(release_root)


def test_release_version_rejects_stale_lockfile(release_root):
    lock = release_root / "Cargo.lock"
    lock.write_text(lock.read_text().replace("0.1.0", "0.0.9", 1))
    with pytest.raises(AssertionError, match="lockfile version differs"):
        release_version(release_root)


@pytest.fixture
def bundle(tmp_path):
    for platform in PLATFORMS:
        tag = "cp313-abi3-" + platform
        with zipfile.ZipFile(tmp_path / f"gxwlib-0.1.0-{tag}.whl", "w") as archive:
            archive.writestr(
                "gxwlib-0.1.0.dist-info/WHEEL", "Wheel-Version: 1.0\nTag: " + tag + "\n"
            )
    # This helper validates inventory and hashes; check_distribution separately
    # validates real archive contents, metadata, notices and RECORD integrity.
    (tmp_path / "gxwlib-0.1.0.tar.gz").write_bytes(b"synthetic sdist inventory entry")
    return tmp_path


def test_release_checksums_cover_every_artifact(bundle):
    checksums = check_artifacts(bundle, "0.1.0")
    lines = checksums.splitlines()
    assert len(lines) == 5
    for line in lines:
        digest, filename = line.split("  ")
        assert digest == hashlib.sha256((bundle / filename).read_bytes()).hexdigest()
    assert [line.split("  ")[1] for line in lines] == sorted(p.name for p in bundle.iterdir())


def test_release_bundle_rejects_missing_platform(bundle):
    (bundle / "gxwlib-0.1.0-cp313-abi3-win_amd64.whl").unlink()
    with pytest.raises(AssertionError, match="missing=.*win_amd64"):
        check_artifacts(bundle, "0.1.0")


def test_release_bundle_rejects_wrong_linux_floor(bundle):
    wheel = bundle / "gxwlib-0.1.0-cp313-abi3-manylinux_2_28_x86_64.whl"
    wheel.rename(wheel.with_name(wheel.name.replace("2_28", "2_34")))
    with pytest.raises(AssertionError, match="unexpected=.*2_34"):
        check_artifacts(bundle, "0.1.0")


def test_release_bundle_rejects_unexpected_payload(bundle):
    (bundle / "private.gxw").write_bytes(b"synthetic unexpected file")
    with pytest.raises(AssertionError, match="unexpected=.*private.gxw"):
        check_artifacts(bundle, "0.1.0")


def test_release_bundle_rejects_renamed_incompatible_wheel(bundle):
    path = bundle / "gxwlib-0.1.0-cp313-abi3-win_amd64.whl"
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr("gxwlib-0.1.0.dist-info/WHEEL", "Tag: cp314-cp314-win_amd64\n")
    with pytest.raises(AssertionError, match="wheel tag mismatch"):
        check_artifacts(bundle, "0.1.0")
