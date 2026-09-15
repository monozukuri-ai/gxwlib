"""Run isolated scans with explicit virtual CPU and timing parameters."""

from . import Simulator, compile_program
from .analysis_cli import load_instructions


def register(sub):
    command = sub.add_parser("simulate", help="run isolated scans with a chosen CPU profile")
    command.add_argument("path")
    command.add_argument("--profile", choices=["fx3g"], required=True)
    command.add_argument("--scan-period-ns", type=int, required=True)
    command.add_argument(
        "--timer-phase-ns", type=int, default=0, help="virtual clock phase, 0 <= ns < 100000000"
    )
    command.add_argument("--input-format", choices=["gxw", "csv"], default="gxw")
    command.add_argument("--program", type=int, default=0)
    command.add_argument("--scans", type=int, default=1)
    command.add_argument(
        "--input",
        action="append",
        default=[],
        metavar="X0=1",
        help="held X input; repeat for multiple inputs",
    )
    command.add_argument(
        "--initial", action="append", default=[], metavar="M0=1", help="initial Y/M state"
    )
    command.add_argument(
        "--watch", action="append", help="device to trace; default: all referenced/updated devices"
    )


def assignments(items):
    result = {}
    for item in items:
        key, separator, value = item.partition("=")
        if not separator or not key or value.lower() not in {"0", "1", "false", "true"}:
            raise ValueError(f"expected DEVICE=0 or DEVICE=1: {item}")
        if key in result:
            raise ValueError(f"duplicate device assignment: {key}")
        result[key] = value.lower() in {"1", "true"}
    return result


def run(args):
    if not 0 < args.scan_period_ns < 2**64 or not 0 <= args.scans < 2**64:
        raise ValueError("scan-period-ns must be a positive u64 and scans a nonnegative u64")
    if not 0 <= args.timer_phase_ns < 100_000_000:
        raise ValueError("timer-phase-ns must be in 0..100000000")
    program = load_instructions(args.path, args.input_format, args.program, "fx")
    compiled = compile_program(program, profile=args.profile)
    simulator = Simulator(
        compiled,
        scan_period_ns=args.scan_period_ns,
        initial_state=assignments(args.initial),
        timer_phase_ns=args.timer_phase_ns,
    )
    simulator.set_inputs(assignments(args.input))
    try:
        trace = simulator.run(args.scans, watch=args.watch)
    except KeyboardInterrupt as error:
        if (trace := getattr(error, "trace", None)) is not None:
            print(trace.to_json())
            return 130
        raise
    print(trace.to_json())
    return 0
