"""Check local public-document links and keep excluded material out of user guides."""

import argparse
import json
import re
import tomllib
from pathlib import Path
from urllib.parse import unquote, urlsplit


def anchors(text):
    counts = {}
    result = set()
    for title in re.findall(r"^#{1,6}\s+(.+?)\s*#*\s*$", text, re.MULTILINE):
        slug = re.sub(r"[^\w\- ]", "", title.lower()).replace(" ", "-")
        count = counts.get(slug, 0)
        counts[slug] = count + 1
        result.add(f"{slug}-{count}" if count else slug)
    return result


def check(root):
    config = tomllib.loads((root / "pyproject.toml").read_text(encoding="utf-8"))
    excluded = {
        pattern.split("/", 1)[0]
        for pattern in config["tool"]["maturin"]["exclude"]
        if not any(c in pattern.split("/", 1)[0] for c in "*?[")
    }
    documents = [
        root / "README.md",
        *sorted((root / "docs").rglob("*.md")),
        root / "scripts/README.md",
    ]
    links = 0
    for path in documents:
        text = path.read_text(encoding="utf-8")
        if path == root / "README.md" or path.is_relative_to(root / "docs"):
            for part in excluded:
                assert not re.search(rf"(?<![\w.-]){re.escape(part)}/", text), path
            assert not re.search(r"(?:/home/|/Users/|file://|\bP[0-6]\b)", text), path
        # Ignore fenced examples when recognizing Markdown links and heading anchors.
        prose = re.sub(r"^```[^\n]*\n.*?^```\s*$", "", text, flags=re.MULTILINE | re.DOTALL)
        for target in re.findall(r"\[[^\]\n]*\]\(([^)\n]+)\)", prose):
            target = target.strip()
            target = target[1 : target.index(">")] if target.startswith("<") else target.split()[0]
            url = urlsplit(target)
            if url.scheme in {"https", "http", "mailto"}:
                continue
            assert not url.scheme and not url.netloc, (path, target)
            decoded = unquote(url.path)
            assert not Path(decoded).is_absolute(), (path, target)
            resolved = (path.parent / decoded).resolve() if decoded else path
            assert resolved.is_relative_to(root), (path, target)
            assert not set(resolved.relative_to(root).parts) & excluded, (path, target)
            assert resolved.exists(), (path, target)
            if url.fragment and resolved.suffix == ".md":
                content = resolved.read_text(encoding="utf-8")
                assert unquote(url.fragment) in anchors(content), (path, target)
            links += 1
    return {"public_documents": len(documents), "local_links": links, "status": "passed"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    print(json.dumps(check(args.root.resolve())))


if __name__ == "__main__":
    main()
