"""Rebuild a wheel from an extracted sdist, then cold-install that wheel."""

import argparse
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

from check_cold_install import run


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dist", type=Path, default=Path("dist"))
    args = parser.parse_args()
    (source,) = args.dist.resolve().glob("*.tar.gz")
    with tempfile.TemporaryDirectory(prefix="gxw-sdist-") as directory:
        temp = Path(directory)
        with tarfile.open(source) as archive:
            archive.extractall(temp, filter="data")
        (root,) = temp.iterdir()
        output = temp / "wheels"
        subprocess.run(
            [
                sys.executable,
                "-m",
                "maturin",
                "build",
                "--release",
                "--locked",
                "--out",
                str(output),
                "--interpreter",
                sys.executable,
            ],
            cwd=root,
            check=True,
        )
        (wheel,) = output.glob("*.whl")
        run(wheel, root)
    print("Sdist rebuild, cold install and subprocess shutdown passed.")


if __name__ == "__main__":
    main()
