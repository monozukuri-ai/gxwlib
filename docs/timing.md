# Timers, counters and virtual time

The FX3G simulator supports constant-preset, coil-evaluated timers and 16-bit
up counters. The clock and retention policy are explicit simulation parameters;
GXW CPU parameters and oscillator phase are not inferred. Native GXW T/C operand
encodings remain unverified, so timer/counter programs must be imported through
the UTF-16LE instruction CSV parser.

```python
import gxwlib

p = gxwlib.load_csv("timers.csv", device_profile="fx")
sim = gxwlib.Simulator(
    gxwlib.compile_program(p, profile="fx3g"),
    scan_period_ns=7_500_000,
    timer_phase_ns=500_000,
)
sim.set_inputs({"X0": True})
trace = sim.run(5, watch=["T200", "T256"])
print([sample.timers["T200"].value for sample in trace.samples])
print(trace.final_snapshot.timer("T200").done)
sim.set_scan_period_ns(2_500_000)
state = sim.step(trace_instructions=True)
```

## Executable subset

| Device | Base | OFF input | Default STOP retention |
|---|---:|---|---|
| T0–T191 | 100 ms | Clear value/contact | Clear |
| T200–T245 | 10 ms | Clear value/contact | Clear |
| T250–T255 | 100 ms | Retain value/contact | Retain |
| T256–T319 | 1 ms | Clear value/contact | Clear |
| C0–C15 | Rising input edge | Retain value/contact | Clear |
| C16–C199 | Rising input edge | Retain value/contact | Retain |

T/C contacts work with LD/LDI, AND/ANI and OR/ORI. `OUT T… K…`, `OUT C… K…`
accept literal K0–K32767, and `RST T…`/`RST C…` clear their state. Each device
can have at most one OUT and one RST; references without an OUT remain valid,
with `preset=None` and zero state. Dynamic D presets, direct numeric memory
injection, duplicate coils/reset instructions, T192–T199 routine timers,
T246–T249 interrupt timers, C200+ bidirectional/high-speed counters, special
relays and parameter overrides are rejected before execution.

The distinction between timer ranges, coil evaluation, zero presets, counter
edges and default retained areas follows the
[Mitsubishi FX programming manual JY997D16601R](https://www.mitsubishielectric.com/dl/fa/document/manual/plc_fx/jy997d16601/jy997d16601r.pdf),
printed pp. 38, 41, 86, 98–100, 103–104 and 215. The scheduling and reset-order
rules below define the deterministic model used to apply those descriptions.
They have not been compared with a captured GX Simulator2 or physical PLC trace.

## Clock, evaluation and first scan

`timing_model="scan_start_coil_clock_v1"` executes the instruction sequence at
the current virtual scan-start time, then refreshes Y and advances time by the
specified period. Individual instruction execution durations are zero in this
model. The initial state represents the start of the first program scan; the
CPU's initial END processing, startup latency and input filters are not timed.

The virtual oscillator has pulses at `n × time_base_ns - timer_phase_ns`.
`timer_phase_ns` defaults to zero and must satisfy `0 <= phase < 100_000_000`.
Each timer reduces this common offset modulo its own base. Count pulses in the
interval `(previous evaluation time, current evaluation time]` only when the
preceding coil enabled the timer. Integer arithmetic preserves sub-millisecond
scan periods and avoids floating-point rounding. A period change affects future
scan lengths; it does not restart the oscillator or reinterpret previous time.

The first ON starts counting and receives no earlier time. At a later OUT,
the timer updates its value and contact, saturating at the preset. An OUT OFF
clears a general timer. A retentive timer accounts for the preceding enabled
interval, then keeps its value/contact while OFF. Its retained quantity is the
integer pulse count, not a separate sum of fractional ON intervals.

Consequently, a contact preceding OUT sees the previous coil evaluation; a
contact following OUT sees the new result. A snapshot's `time_ns` marks the end
of the completed scan, while its timer value comes from the coil evaluation at
that scan's start. Time passage alone does not refresh these timer contacts.
`elapsed_ns` is `value × time_base_ns`; `phase_ns` describes the oscillator phase
at the snapshot/event timestamp, not a timer's accumulated fractional time.

For a timer with K0, the first ON leaves the contact OFF and a subsequent enabled
coil evaluation sets it ON. For a counter, K0 behaves like K1: its first eligible
rising edge sets value 1 and the contact ON. The configured preset remains zero
in the trace.

## Counter edges and reset ordering

The counter remembers the last OUT input. It increments on False → True, holds
on continuous True or False, and stops counting at the effective preset. It
continues tracking input transitions after completion. `rising_edge` records an
observed edge even when reset or saturation prevents an increment.

RST evaluates a reset coil in instruction order. True immediately clears the
value and contact and inhibits counting; a later execution of that RST with
False releases the inhibition. RST preserves the sampled counter input, so
releasing reset while the input remains True does not manufacture an edge.
For timers, reset clears the counting interval; the next enabled interval starts
when OUT executes with reset released. A reset after OUT can clear a value that
earlier contacts already observed. These reset-coil rules define the virtual model,
including simultaneous reset/count and reset-after-OUT programs.

`Simulator.reset()` is a different operation: it clears the entire simulated
state, time and scan count and starts RUN, retaining the chosen scan period and
configured phase. `initial_state` and `set_devices()` accept only Y/M bools;
they cannot set T/C contacts independently of their numeric state.

## STOP/RUN and retained state

```python
sim.stop()
sim.advance_stopped(1_000_000_000)  # clock advances by 1 s; no program scan
assert not sim.running
sim.start()
state = sim.step()
```

`retention_policy="fx3g_default_m8033_off"` selects factory areas with no optional
battery/parameter changes and M8033 OFF. STOP clears X/Y images and external Y,
M0–M383/M1536–M7679, general timers and C0–C15. It preserves M384–M1535,
T250–T255, C16–C199 and the retained devices' input/reset/contact state. Pending
external X inputs remain available for the next RUN scan. A retained counter
whose input stays ON across STOP/RUN does not gain an edge; a cleared counter
can count its first ON on restart.

STOP retains the last evaluated timer value and discards any unevaluated clock
interval. Stopped time adds no timer pulses. `advance_stopped()` changes the
oscillator phase and time without changing scan count, contacts or outputs.
`start()` re-anchors timer accounting at that time. Repeated stop/start calls
in the same mode are idempotent. `step()` and `run()` reject STOP mode, including
`run(0)`. Timing/mode operations use the same nonblocking instance lock as scans.

This is not power-cycle, EEPROM writing/endurance or forced-output emulation.
It does not recover the original project's retention settings.

## Numeric state

`ScanSnapshot.timer(name)` / `.counter(name)` return immutable typed state.
Snapshots expose referenced T/C states through `.timers` / `.counters` maps.
Unreferenced supported devices can be queried and return zero with `preset=None`.
Timer state includes its base, evaluation point, retained attribute, input and
previous input, preset, value, quantized elapsed time, phase, contact and reset
status. Counter state includes input history and the last observed rising edge.

An instruction event includes `time_ns` and the affected `.timer` or `.counter`
state after that instruction, alongside the existing original source range.
In `run(watch=["T0", "C0"])`, `sample.values` contains their contact bools and
`sample.timers` / `.counters` contain numeric state. Each T/C watch costs 16
trace-value units per scan; each ordinary bit costs one. `watch=[]` collects
no per-scan device state; the final snapshot still contains referenced state.
Simulation JSON schema version is 2; instruction/analysis JSON is unchanged.

```sh
gxw simulate timers.csv --input-format csv --profile fx3g \
  --scan-period-ns 7500000 --timer-phase-ns 500000 --input X0=1 --scans 5 \
  --watch T200 --watch T256
```
