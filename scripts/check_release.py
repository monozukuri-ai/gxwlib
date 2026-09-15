"""Validate a stable release version, the platform wheel set and its checksums."""

import argparse
import json
import re
import tomllib
import zipfile
from email.parser import BytesParser
from hashlib import sha256
from pathlib import Path

PLATFORMS = {
    "manylinux_2_28_x86_64",
    "win_amd64",
    "macosx_11_0_arm64",
    "macosx_11_0_x86_64",
}


def release_version(root, tag=""):
    manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    workspace = manifest["workspace"]
    version = workspace["package"]["version"]
    assert re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", version), (
        "release versions must use MAJOR.MINOR.PATCH"
    )
    assert not tag or tag == f"v{version}", f"tag {tag!r} must match v{version}"
    assert workspace["dependencies"]["gxw-core"]["version"] == "=" + version
    locked = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))["package"]
    for crate in ["gxw-core", "gxw-python"]:
        (package,) = [p for p in locked if p["name"] == crate and not p.get("source")]
        assert package["version"] == version, f"{crate} lockfile version differs"
    return version


def check_artifacts(directory, version):
    expected = {f"gxwlib-{version}-cp313-abi3-{platform}.whl" for platform in PLATFORMS}
    expected.add(f"gxwlib-{version}.tar.gz")
    paths = sorted(directory.iterdir())
    actual = {p.name for p in paths}
    assert actual == expected, (
        f"release bundle mismatch: missing={sorted(expected - actual)}, "
        f"unexpected={sorted(actual - expected)}"
    )
    lines = []
    for path in paths:
        assert path.is_file() and not path.is_symlink(), path
        if path.suffix == ".whl":
            with zipfile.ZipFile(path) as archive:
                (wheel_name,) = [n for n in archive.namelist() if n.endswith(".dist-info/WHEEL")]
                metadata = BytesParser().parsebytes(archive.read(wheel_name))
                tag = path.name.removeprefix(f"gxwlib-{version}-").removesuffix(".whl")
                assert metadata.get_all("Tag") == [tag], f"wheel tag mismatch: {path.name}"
        lines.append(f"{sha256(path.read_bytes()).hexdigest()}  {path.name}\n")
    return "".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", default="", help="require the matching vMAJOR.MINOR.PATCH tag")
    parser.add_argument("--dist", type=Path, help="require the full four-wheel and sdist set")
    parser.add_argument("--checksums", type=Path, help="write SHA256SUMS outside --dist")
    args = parser.parse_args()
    if args.checksums and not args.dist:
        parser.error("--checksums requires --dist")
    root = Path(__file__).resolve().parents[1]
    version = release_version(root, args.tag)
    if args.dist:
        checksums = check_artifacts(args.dist, version)
        if args.checksums:
            assert not args.checksums.resolve().is_relative_to(args.dist.resolve()), (
                "checksums must be outside the package directory"
            )
            args.checksums.write_text(checksums, encoding="utf-8", newline="\n")
    print(
        json.dumps({"version": version, "tag": args.tag or None, "bundle_checked": bool(args.dist)})
    )


if __name__ == "__main__":
    main()
