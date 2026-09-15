# Scan simulation

`compile_program()` validates an `InstructionProgram` and creates immutable
executable code. `Simulator` runs one selected program under an explicit FX3G
profile, with integer virtual time, bit instructions and a bounded timer/counter
subset. This models one selected program. GX Simulator2 compatibility and
whole-project execution have not been established. It has no hardware, network
or wall-clock I/O. See [timing and STOP/RUN](timing.md) for the execution model.

## Start from GXW or CSV

```python
import gxwlib

project = gxwlib.load_project("project.gxw")
program = gxwlib.decode_program(project, 0, device_profile="fx")
# Or: program = gxwlib.load_csv("MAIN.csv", device_profile="fx")
compiled = gxwlib.compile_program(program, profile="fx3g")
sim = gxwlib.Simulator(compiled, scan_period_ns=10_000_000)
sim.set_inputs({"X0": True, "X1": False})
state = sim.step()
print(state.scan, state.time_ns, state.output("Y0"))
trace = sim.run(100, watch=["X0", "Y0", "M0"])
for sample in trace.samples:
    print(sample.scan, sample.time_ns, dict(zip(trace.watch, sample.values)))
```

The caller selects the virtual CPU profile; the package does not infer an FX3G
CPU from a filename or incomplete project metadata. Execution order is the
selected program's instruction order, ending in exactly one END. There is no
implicit ordering of multiple programs, subroutines or interrupts. Unrelated
unresolved container metadata does not block a selected, fully decoded program.

Compilation checks supported instruction/operand forms, original source ranges,
END placement, stack structure and device ranges. Unknown instructions, opaque
code and unresolved operands cannot execute. No fallback converts them to NOP.
A supported syntax result alone is insufficient for compilation. For example,
an OUT T0 D0 CSV instruction can be parsed, but its dynamic preset cannot execute.

Supported operations: LD/LDI, AND/ANI, OR/ORI, OUT, SET/RST, ANB/ORB,
MPS/MRD/MPP and END, restricted to these devices:

| Device | Virtual image range | Program use |
|---|---|---|
| X | X0–X177, octal | Read |
| Y | Y0–Y177, octal | Read and write |
| M | M0–M7679, decimal | Read and write |
| T | T0–T191, T200–T245, T250–T319 | Contact, constant-preset OUT, RST |
| C | C0–C199 | Contact, constant-preset OUT, RST |

These are device-image ranges, not a detected installation's physical I/O
allocation or total connected I/O capacity. Special M8000+ devices, D words,
routine/interrupt timers, 32-bit/high-speed counters, indirect/indexed addressing,
pulses, interrupts and other CPU profiles are rejected. The profile and bit instruction rules refer to the
[Mitsubishi FX programming manual JY997D16601R](https://www.mitsubishielectric.com/dl/fa/document/manual/plc_fx/jy997d16601/jy997d16601r.pdf),
printed pp. 86, 170, 200–203, 215 and 218. Input filters, physical delays,
communication/link writers, forced devices and watchdog behavior are not modeled.

## Input, memory and output timing

Each `step()` latches pending X inputs, executes instructions in order, then
commits the Y image to external outputs and advances virtual time by
`scan_period_ns`. There is no wall-clock sleep. The specified period is a model
parameter, not a measured PLC scan period.

Bit OUT updates the image immediately, including writing False when its condition
is False. SET/RST update their target only when their condition is True; otherwise
the previous value persists. Subsequent contacts see image updates in the same
scan. MPS/MRD/MPP preserve evaluated results, including results read before a
later device write. Runtime executes the instruction sequence directly; it does
not re-read every contact in the reconstructed display's output tiles.

For the manual's double-coil example, `X1=True, X2=False` causes the first
`OUT Y3` to set the image, `LD Y3; OUT Y4` to see that True value, and the second
`OUT Y3` to clear it. The final external outputs are `Y3=False, Y4=True`.

`set_inputs()` patches pending X values; unspecified inputs retain their pending
values. They become visible to contacts at the next scan's input latch.
`set_devices()` patches Y/M image values between operations, for example to
represent an external writer. It does not immediately refresh external Y outputs.
Concurrent writes during a scan or `run()` are rejected.

```python
sim.set_inputs({"X0": True})
# sim.snapshot().get("X0") still reports the previous input latch.
sim.set_devices({"M100": True})
state = sim.step(trace_instructions=True)
for event in state.instruction_trace:
    print(event.instruction, event.opcode, event.accumulator,
          event.read_value, event.write_value, event.external_output)
    original = state.compiled.program.read_source(event.source)
```

`read_value` is the bit before contact inversion. `write_value=None` means no
contact-bit write occurred, including an inactive SET/RST. An inactive T/C RST
can still release its reset status; the numeric state records that change. `external_output` reports the
previous committed Y value for a Y operand while instructions execute; final
Y refresh is represented by the completed snapshot. This makes intermediate
writes and final outputs distinguishable.

## Snapshots, initialization and reset

`ScanSnapshot.get("Y0")` reads the device image; `output("Y0")` reads the committed
external output. `devices` and `outputs` include referenced or explicitly updated
devices. `get()` can read any supported bit, including an unreferenced bit.
A missing entry in the displayed maps has its initialized False value.
Snapshots retain independent state and source ownership. Changing returned Python
maps, resetting a simulator or deleting the input file cannot change old snapshots.

Every simulator starts with zeroed images and zero time. Supply
`initial_state={"M0": True, "Y0": True}` to seed Y/M image values. External outputs
still start False until the first scan commits them; supply X via `set_inputs()`.
Y/M values persist across RUN scans until overwritten. [STOP/RUN](timing.md)
applies the explicit FX3G default retention policy.

`reset(initial_state=...)` clears images, pending/latched X inputs, external
outputs, scan count and time, then applies the new seed. It is an explicit model
reset. It also clears T/C states, starts RUN and preserves the selected phase
and current scan period. Use `stop()`/`start()` for the separately specified mode
transitions. Power-cycle and EEPROM/battery behavior are not modeled.

Updates validate every assignment before mutating state. Values must be Python
bools, and aliases referring to the same device (for example `X0` and `x00`)
are rejected within one update/watch list. Unrecognized or out-of-range bits are
errors; they are not silently created.

## Batched execution, interruption and limits

`run(scans, watch=...)` returns `ScanTrace`: normalized `watch` names, one
`ScanSample.values` list per completed scan, and `final_snapshot`. Watching a T/C
also includes its typed numeric state in the sample's `timers`/`counters` maps. Without a watch
list, referenced/explicitly updated devices are watched. `watch=[]` retains scan
numbers and times without device values. Each trace/snapshot records the profile,
execution scope, source SHA-256, scan period, configured timer phase, timing model
and retention policy. Simulation JSON schema version is 2. The source-backed
`CompiledProgram` remains accessible through a snapshot's `compiled` property.

Rust runs outside the Python GIL. A whole operation holds the instance's mutex,
using nonblocking acquisition. Conflicting state operations raise
`SimulationError` with `code="GXW_SIM_BUSY"`. Immutable `compiled` and the `busy`
observation remain available. The mutable scan period and mode properties use
the same lock; change the period with `set_scan_period_ns()`. Separate instances share
compiled code but never share mutable device state.

Python signals are checked between complete scan chunks, at most 256 scans or
approximately 16,384 instructions per chunk (at least one whole scan).
KeyboardInterrupt preserves the completed prefix:

```python
try:
    trace = sim.run(100_000, watch=["Y0"])
except KeyboardInterrupt as interrupted:
    trace = interrupted.trace
    print(interrupted.completed_scans, trace.final_snapshot.scan)
# The lock is released; subsequent step()/run() continues after the last full scan.
```

The prefix has `interrupted=True`, `completed_scans` and `requested_scans`.
If the signal arrives before execution, completed_scans may be zero; it can also
arrive at the final boundary. An in-progress scan is not returned as complete.
No snapshot is rolled back after a completed scan. The Rust counterpart is
`Simulator::run_interruptible`, whose cancellation callback runs at the same
boundaries and returns a partial `ScanTrace`.

`SimulationLimits` defaults:

| Limit per operation | Default | Configuration ceiling |
|---|---:|---:|
| Scans | 100,000 | 1,000,000 |
| Instruction executions, including END | 100,000,000 | 1,000,000,000 |
| Trace units (bit = 1, T/C state = 16, per scan) | 1,000,000 | 10,000,000 |
| Instruction events in `step(trace_instructions=True)` | 100,000 | 1,000,000 |

Compilation defaults to 100,000 instructions and allows a selected limit up to
1,000,000. Block/MPS stack limits are fixed at 8/11 results for this profile.
Compile/trace/operation limits raise `ResourceLimitError`.
Virtual time and scan counters use checked u64 arithmetic. An overflowing request
or invalid update fails before executing any scan or changing input latches.
Simulation errors expose `code` and `instruction` (which may be None);
`SimulationError` derives from `GxwError`.

## CLI

```sh
gxw simulate MAIN.csv --input-format csv \
  --profile fx3g --scan-period-ns 10000000 --scans 3 \
  --input X1=1 --input X2=0 --watch Y3 --watch Y4 > trace.json
```

Use `--program INDEX` for a GXW POU, `--initial M0=1` to seed Y/M, and repeated
`--input`/`--watch` options as needed. CLI inputs are held throughout the batch;
use `set_inputs()` between `step()` calls for changing stimuli.
`--timer-phase-ns` selects the virtual oscillator offset. Output is JSON.
Exit 0 means the requested batch finished; 1 is an input/compile/runtime limit
error; 2 is command-line usage error. An interruption during `run()` emits the
completed trace and exits 130. A compilation failure does not emit simulated data.

See [timers and counters](timing.md) for virtual clock behavior, retention and
numeric traces.
