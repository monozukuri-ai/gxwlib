"""Collect verbatim license notices for every registry package in Cargo.lock."""

import json
import subprocess
from pathlib import Path


def main():
    root = Path(__file__).resolve().parents[1]
    metadata = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--locked", "--format-version", "1"], cwd=root, encoding="utf-8"
        )
    )
    output = root / "LICENSES"
    output.mkdir(exist_ok=True)
    inventory = []
    for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
        if package["source"] is None:
            continue
        directory = Path(package["manifest_path"]).parent
        files = sorted(
            p
            for p in directory.iterdir()
            if p.is_file()
            and p.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE"))
        )
        assert files, f"no license text for {package['name']}"
        name = f"{package['name']}-{package['version']}"
        text = f"{name}\nSource: {package['source']}\nLicense: {package['license']}\n"
        for source in files:
            text += f"\n--- {source.name} ---\n" + source.read_text(encoding="utf-8") + "\n"
        (output / f"{name}.txt").write_text(text, encoding="utf-8")
        inventory.append(
            {
                "name": package["name"],
                "version": package["version"],
                "license": package["license"],
                "notice": f"{name}.txt",
            }
        )
    (output / "inventory.json").write_text(json.dumps(inventory, indent=2) + "\n", encoding="utf-8")
    print(f"Collected notices for {len(inventory)} registry packages (all locked targets).")


if __name__ == "__main__":
    main()
