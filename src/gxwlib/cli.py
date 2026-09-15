"""The gxw command line entry point."""

import argparse
import sys

from . import (
    GxwError,
    __version__,
    analysis_cli,
    decode_program,
    inspect,
    load_csv,
    load_project,
    simulation_cli,
    structured_cli,
)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="gxw", description="Read, analyze and display GX Works2 projects"
    )
    parser.add_argument("--version", action="version", version=f"gxwlib {__version__}")
    sub = parser.add_subparsers(dest="command", required=True)
    command = sub.add_parser("inspect", help="list streams and logical objects")
    command.add_argument("path")
    command.add_argument("--json", action="store_true", help="emit the versioned JSON index")
    raw = sub.add_parser("parse", help="show raw POU tokens and framing diagnostics")
    raw.add_argument("path")
    raw.add_argument("--json", action="store_true", help="emit the versioned raw project JSON")
    for name, help_text in [
        ("decode", "decode the supported instruction subset"),
        ("csv", "read GX Works2 list CSV"),
    ]:
        command = sub.add_parser(name, help=help_text)
        command.add_argument("path")
        command.add_argument("--device-profile", choices=["fx"], required=True)
        command.add_argument("--json", action="store_true")
    analysis_cli.register(sub)
    simulation_cli.register(sub)
    structured_cli.register(sub)
    args = parser.parse_args(argv)
    try:
        if args.command in structured_cli.COMMANDS:
            return structured_cli.run(args)
        if args.command == "simulate":
            return simulation_cli.run(args)
        if args.command in analysis_cli.COMMANDS:
            return analysis_cli.run(args)
        if args.command in {"decode", "csv"}:
            if args.command == "csv":
                programs = [load_csv(args.path, device_profile=args.device_profile)]
            else:
                raw_project = load_project(args.path)
                programs = [
                    decode_program(raw_project, i, device_profile=args.device_profile)
                    for i in range(len(raw_project.programs))
                ]
            if args.json:
                # Both commands use a list, including when the GXW has no POU.
                print("[" + ",\n".join(p.to_json() for p in programs) + "]")
            else:
                for program in programs:
                    print(f"{program.logical_name}: complete={program.complete}, device_profile=fx")
                    for instruction in program.instructions:
                        operands = []
                        for operand in instruction.operands:
                            value = operand.value
                            if operand.original_text is not None:
                                text = operand.original_text
                            elif value.kind == "device":
                                address = (
                                    format(value.address, "o")
                                    if value.radix == 8
                                    else str(value.address)
                                )
                                text = value.device + address
                            elif value.kind == "constant":
                                text = (
                                    f"H{value.value:X}" if value.radix == 16 else f"K{value.value}"
                                )
                            else:
                                text = "<?>"
                            operands.append(text)
                        suffix = "" if instruction.supported else " [unsupported]"
                        print(
                            f"  {instruction.ordinal}: {instruction.mnemonic or '<?>'} {' '.join(operands)}{suffix}"
                        )
                    for diagnostic in program.diagnostics:
                        print(f"{diagnostic.code}: {diagnostic.message}")
            return 0 if programs and all(p.complete for p in programs) else 3
        if args.command == "parse":
            project = load_project(args.path)
            if args.json:
                print(project.to_json())
            else:
                print(f"Raw framing only: semantic_status={project.semantic_status}")
                for program in project.programs:
                    print(
                        f"{program.logical_name}: {program.framing_status}, profile={program.profile}"
                    )
                    for token in program.tokens:
                        print(f"  0x{token.source.offset:08x}  {token.data_hex}")
                for diagnostic in project.diagnostics:
                    print(f"{diagnostic.severity}: {diagnostic.code}: {diagnostic.message}")
            return 0 if project.framing_complete else 3
        project = inspect(args.path)
        if args.json:
            print(project.to_json())
        else:
            print(f"{args.path}: {project.size} bytes, SHA-256 {project.sha256}")
            for container in project.containers:
                name = "/".join(container.path) or "outer"
                print(f"[{name}] CFB v{container.cfb_version}: {len(container.streams)} streams")
                for stream in container.streams:
                    print(f"  {'/'.join(stream.path)}\t{stream.size}\t{stream.sha256}")
            print(f"Logical objects: {len(project.logical_objects)}")
            for item in project.logical_objects:
                location = item.location
                target = "/".join(location.container + location.path) if location else "unresolved"
                print(f"  {item.id}: {item.logical_name} -> {target}")
            for diagnostic in project.diagnostics:
                print(f"{diagnostic.severity}: {diagnostic.code}: {diagnostic.message}")
    except (GxwError, OSError, ValueError) as error:
        print(f"gxw: {error}", file=sys.stderr)
        return 1
    return 0
