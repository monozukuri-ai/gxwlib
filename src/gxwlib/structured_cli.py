"""Explicitly selected structured POU/declaration inspection."""

from pathlib import Path

from . import decode_declarations, decode_structured, load_project, render_structured

COMMANDS = {"structured", "declarations", "render-structured"}


def register(sub):
    for name in sorted(COMMANDS):
        p = sub.add_parser(name, help=name.replace("-", " "))
        p.add_argument("path")
        if name == "declarations":
            p.add_argument("--logical-index", type=int, required=True)
        else:
            p.add_argument("--program-index", type=int, default=0)
        if name == "render-structured":
            p.add_argument("--format", choices=["svg", "html"], default="svg")
            p.add_argument("--output", type=Path)
        else:
            p.add_argument("--json", action="store_true")


def run(args):
    project = load_project(args.path)
    if args.command == "declarations":
        result = decode_declarations(project, args.logical_index)
    else:
        result = decode_structured(project, args.program_index)
    if args.command == "render-structured":
        doc = render_structured(result)
        text = doc.to_html() if args.format == "html" else doc.svg
        if args.output:
            # A read-only command must not replace its source project.
            if args.output.resolve() == Path(args.path).resolve() or (
                args.output.exists() and args.output.samefile(args.path)
            ):
                raise ValueError("output must differ from input")
            args.output.write_text(text, encoding="utf-8")
        else:
            print(text, end="")
    elif args.json:
        print(result.to_json())
    else:
        print(f"{result.logical_name}: structure_complete={result.structure_complete}")
        if args.command == "structured":
            print("Stored geometry only; execution not evaluated")
            for b in result.blocks:
                print(
                    f"  block {b.index}: {len(b.nodes)} nodes, {len(b.wires)} wires, {len(b.nets)} nets"
                )
        else:
            for row in result.rows:
                print(f"  {row.name}: {row.data_type} = {row.initial_value}")
        for d in result.diagnostics:
            print(f"{d.code}: {d.message}")
    return 0 if result.structure_complete else 3
