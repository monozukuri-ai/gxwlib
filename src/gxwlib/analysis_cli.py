"""CLI adapters for Rust analysis and rendering of one explicitly selected program."""

from pathlib import Path

from . import (
    AnalysisOptions,
    analyze,
    decode_program,
    diff_programs,
    load_csv,
    load_project,
    render_ladder,
    viewer,
)

COMMANDS = {"analyze", "render", "diff"}


def register(sub):
    for name, description in [
        ("analyze", "list device reads/writes and structural diagnostics"),
        ("render", "open a local ladder viewer or export SVG/HTML"),
        ("diff", "compare normalized instruction sequences"),
    ]:
        command = sub.add_parser(name, help=description)
        command.add_argument("path")
        command.add_argument("--device-profile", choices=["fx"], required=True)
        command.add_argument("--input-format", choices=["gxw", "csv"], default="gxw")
        command.add_argument("--program", type=int, default=0, help="zero-based POU index")
        if name != "render":
            command.add_argument("--json", action="store_true")
        if name != "diff":
            command.add_argument("--external-write", action="append", default=[], metavar="DEVICE")
            command.add_argument("--no-inputs-external", action="store_true")
        if name == "render":
            command.add_argument(
                "--format", choices=["svg", "html"], help="export to stdout or --output"
            )
            command.add_argument(
                "--output", type=Path, help="write a new file (default format: SVG)"
            )
            command.add_argument("--force", action="store_true", help="replace an existing output")
            command.add_argument("--max-elements", type=int, default=20_000)
            command.add_argument("--max-output-bytes", type=int, default=16_777_216)
            viewer.register_options(command)
        if name == "diff":
            command.add_argument("right")
            command.add_argument(
                "--right-format", choices=["gxw", "csv"], help="default: input-format"
            )
            command.add_argument("--right-program", type=int, default=0)
            command.add_argument("--max-cells", type=int, default=1_000_000)


def load_instructions(path, file_format, index, profile):
    if index < 0:
        raise ValueError("program index must be nonnegative")
    if file_format == "csv":
        if index != 0:
            raise ValueError("CSV contains one program; --program must be 0")
        return load_csv(path, device_profile=profile)
    return decode_program(load_project(path), index, device_profile=profile)


def run(args):
    serving = viewer.wants_server(args) if args.command == "render" else False
    program = load_instructions(args.path, args.input_format, args.program, args.device_profile)
    if args.command == "diff":
        if args.max_cells < 0:
            raise ValueError("max-cells must be nonnegative")
        right = load_instructions(
            args.right,
            args.right_format or args.input_format,
            args.right_program,
            args.device_profile,
        )
        result = diff_programs(program, right, max_cells=args.max_cells)
        if args.json:
            print(result.to_json())
        else:
            print(
                f"complete={result.complete}, different={result.different}, opaque_changed={result.opaque_changed}"
            )
            for h in result.hunks:
                print(
                    f"{h.kind}: left[{h.left_start}:{h.left_start + h.left_count}] right[{h.right_start}:{h.right_start + h.right_count}]"
                )
        return 3 if not result.complete else 4 if result.different else 0
    options = AnalysisOptions(
        external_writes=args.external_write, inputs_external=not args.no_inputs_external
    )
    if args.command == "analyze":
        result = analyze(program, options=options)
        if args.json:
            print(result.to_json())
        else:
            print(f"{program.logical_name}: complete={result.complete}")
            accesses = result.accesses
            for usage in result.devices:

                def sites(indices):
                    return ",".join(str(accesses[i].instruction) for i in indices) or "-"

                print(
                    f"{usage.device.name}: read=#{sites(usage.reads)} write=#{sites(usage.writes)} unknown=#{sites(usage.unknown)} external={usage.external_write}"
                )
            for finding in result.diagnostics:
                print(f"{finding.severity}: {finding.code}: {finding.message}")
    else:
        if args.max_elements < 0 or args.max_output_bytes < 0:
            raise ValueError("render limits must be nonnegative")
        result = render_ladder(
            program,
            options=options,
            max_elements=args.max_elements,
            max_output_bytes=args.max_output_bytes,
        )
        if serving:
            viewer.serve_html(
                result.to_html(), port=args.port or 0, open_browser=not args.no_browser
            )
            return 0 if result.complete else 3
        text = result.to_html() if args.format == "html" else result.svg
        if args.output:
            source = Path(args.path).resolve()
            destination = args.output.resolve()
            if source == destination or (destination.exists() and source.samefile(destination)):
                raise ValueError("output must differ from the input file")
            with args.output.open(
                "w" if args.force else "x", encoding="utf-8", newline=""
            ) as stream:
                stream.write(text)
        else:
            print(text, end="")
    return 0 if result.complete else 3
