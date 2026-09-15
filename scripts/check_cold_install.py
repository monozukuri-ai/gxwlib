"""Install a built wheel into an empty venv and run tests outside the checkout."""

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
import venv
from pathlib import Path


def run(
    wheel,
    root,
    *,
    rust_instructions=None,
    rust_analysis=None,
    rust_simulation=None,
    extra_checks=None,
):
    with tempfile.TemporaryDirectory(prefix="gxw-cold-") as temporary:
        temp = Path(temporary)
        environment = temp / "venv"
        # Match `python -m venv` on POSIX: copied standalone Python binaries can
        # lose their relative shared-library lookup outside the original prefix.
        venv.EnvBuilder(with_pip=True, symlinks=os.name != "nt").create(environment)
        bin_dir = environment / ("Scripts" if os.name == "nt" else "bin")
        python = bin_dir / ("python.exe" if os.name == "nt" else "python")
        env = os.environ.copy()
        env.pop("PYTHONPATH", None)
        env["PYTHONNOUSERSITE"] = "1"
        env["PYTHONUTF8"] = "1"
        compiler_names = ["cargo", "rustc", "cargo.exe", "rustc.exe"]
        paths = [
            p
            for p in env.get("PATH", "").split(os.pathsep)
            if p and not any((Path(p) / name).exists() for name in compiler_names)
        ]
        env["PATH"] = os.pathsep.join([str(bin_dir), *paths])
        subprocess.run(
            [str(python), "-m", "pip", "install", "--no-index", "--no-deps", str(wheel)],
            cwd=temp,
            env=env,
            check=True,
        )
        shutil.copy2(root / "scripts" / "smoke_installed.py", temp / "smoke.py")
        fixture = root / "tests" / "fixtures" / "minimal.gxw"
        subprocess.run(
            [
                str(python),
                str(temp / "smoke.py"),
                "--fixture",
                str(fixture),
                "--raw-fixture",
                str(fixture.with_name("raw.gxw")),
                "--structured-fixture",
                str(fixture.with_name("structured.gxw")),
            ],
            cwd=temp,
            env=env,
            check=True,
        )
        # pytest is a validation tool, installed only after the dependency-free smoke.
        subprocess.run(
            [str(python), "-m", "pip", "install", "pytest>=8,<10"], cwd=temp, env=env, check=True
        )
        shutil.copytree(
            root / "tests", temp / "tests", ignore=shutil.ignore_patterns("__pycache__")
        )
        subprocess.run(
            [str(python), "-m", "pytest", str(temp / "tests" / "python"), "-q"],
            cwd=temp,
            env=env,
            check=True,
        )
        if rust_instructions is not None:
            shutil.copy2(
                root / "scripts" / "check_instruction_parity.py",
                temp / "check_instruction_parity.py",
            )
            roots = [temp / "tests" / "fixtures"]
            for fixture_root in roots:
                subprocess.run(
                    [
                        str(python),
                        str(temp / "check_instruction_parity.py"),
                        "--root",
                        str(fixture_root),
                        "--rust-instructions",
                        str(rust_instructions),
                    ],
                    cwd=temp,
                    env=env,
                    check=True,
                )
        if rust_analysis is not None:
            shutil.copy2(
                root / "scripts" / "check_analysis_parity.py", temp / "check_analysis_parity.py"
            )
            roots = [temp / "tests" / "fixtures"]
            for fixture_root in roots:
                subprocess.run(
                    [
                        str(python),
                        str(temp / "check_analysis_parity.py"),
                        "--root",
                        str(fixture_root),
                        "--rust-analysis",
                        str(rust_analysis),
                    ],
                    cwd=temp,
                    env=env,
                    check=True,
                )
        if rust_simulation is not None:
            shutil.copy2(
                root / "scripts" / "check_simulation_parity.py", temp / "check_simulation_parity.py"
            )
            shutil.copy2(
                root / "scripts" / "check_timing_parity.py", temp / "check_timing_parity.py"
            )
            subprocess.run(
                [
                    str(python),
                    str(temp / "check_timing_parity.py"),
                    "--root",
                    str(temp / "tests" / "fixtures" / "timing"),
                    "--rust-simulation",
                    str(rust_simulation),
                ],
                cwd=temp,
                env=env,
                check=True,
            )
            roots = [temp / "tests" / "fixtures"]
            for fixture_root in roots:
                subprocess.run(
                    [
                        str(python),
                        str(temp / "check_simulation_parity.py"),
                        "--root",
                        str(fixture_root),
                        "--rust-simulation",
                        str(rust_simulation),
                    ],
                    cwd=temp,
                    env=env,
                    check=True,
                )
        if extra_checks is not None:
            # Optional caller-supplied checks run while the isolated environment exists.
            extra_checks(python, temp, env)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dist", type=Path, default=Path("dist"))
    parser.add_argument("--rust-instructions", type=Path)
    parser.add_argument("--rust-analysis", type=Path)
    parser.add_argument("--rust-simulation", type=Path)
    args = parser.parse_args()
    (wheel,) = args.dist.resolve().glob("*.whl")
    run(
        wheel,
        Path(__file__).resolve().parents[1],
        rust_instructions=args.rust_instructions.resolve() if args.rust_instructions else None,
        rust_analysis=args.rust_analysis.resolve() if args.rust_analysis else None,
        rust_simulation=args.rust_simulation.resolve() if args.rust_simulation else None,
    )
    print(f"Cold install and subprocess shutdown passed: {wheel.name} on {sys.version.split()[0]}")


if __name__ == "__main__":
    main()
