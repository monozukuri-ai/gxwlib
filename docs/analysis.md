# Static analysis, ladder display and instruction diff

The Rust core operates on the ordered `InstructionProgram` produced by
[the GXW decoder or CSV reader](instructions.md). Select `device_profile="fx"`
explicitly. This selects device spelling, not an inferred CPU model. Support
covers the same 15 basic opcodes as the decoder, including block and saved-result
operations. [Bit scan simulation](simulation.md) uses a separate validation and
compilation step; an analysis/display result is not itself executable.

## Device references and findings

```python
import gxwlib

raw = gxwlib.load_project("project.gxw")
program = gxwlib.decode_program(raw, 0, device_profile="fx")
# Alternatively: gxwlib.load_csv("MAIN.csv", device_profile="fx")
options = gxwlib.AnalysisOptions(external_writes=["M100", "D10"])
report = gxwlib.analyze(program, options=options)

for device in report.devices:
    for index in device.reads + device.writes + device.unknown:
        access = report.accesses[index]
        print(device.device.name, access.mode, access.instruction, access.operand)
        print(report.program.read_source(access.source))
for finding in report.diagnostics:
    print(finding.code, finding.severity, finding.instruction, finding.message)
print(report.complete)
```

`DeviceUsage.reads/writes/unknown` contain indices into `report.accesses`.
Each access identifies an instruction index and operand index, with the operand's
original byte span. Instruction indices are zero-based positions, distinct from
GX Works2 step numbers. X/Y addresses use octal; M/D/T/C use decimal. Leading
zeros and letter case normalize to one device identity.

This is a **syntactic, single-program** cross-reference: known operands in later
instructions are listed even after unknown control flow or END. Unknown effects
remain `mode="unknown"`; no read/write direction is guessed. An OUT T/C target
is a write and its device preset is a read. This describes device references,
without resolving timer/counter internals or validating execution semantics.

X inputs are externally supplied by default. `inputs_external=False` changes
that assumption; `external_writes=[...]` adds devices supplied by HMI, other
programs or other external sources. A read without a resolved local writer is
informational (`GXW_NO_KNOWN_LOCAL_WRITE`), because retained state and external
writers are possible. Two or more local writes produce `GXW_MULTIPLE_WRITES`,
except an ordinary pair of one SET and one RST. These findings do not decide
whether multiple writes or their ordering are correct for a machine.

`GXW_OPERAND_UNRESOLVED` and `GXW_ACCESS_UNRESOLVED` identify unresolved operands
and instruction effects. Stack diagnostics include `GXW_LOGIC_UNDERFLOW`,
`GXW_BLOCK_UNDERFLOW/OVERFLOW/UNCLOSED` and
`GXW_MPS_UNDERFLOW/OVERFLOW/UNCLOSED`.

`complete=True` means this program has supported syntax, no opaque regions,
and a fully reconstructed connection structure under the selected limits.
Informational findings and multiple-write warnings may still exist. It does
not establish whole-project coverage, execution order, CPU compatibility,
reachability or interlock correctness. Original decoder diagnostics remain
available through `report.program.diagnostics`.

## Ladder graph, SVG, HTML and Notebook

```python
from pathlib import Path

view = gxwlib.render_ladder(program)
Path("ladder.svg").write_text(view.svg, encoding="utf-8")
Path("ladder.html").write_text(view.to_html(), encoding="utf-8")
# In a notebook, make `view` the last expression to display its SVG.
view
```

The standalone HTML includes zoom, pointer/keyboard selection, the original
instruction list and diagnostics. It requires no server or external libraries.
Selecting a contact, coil or control instruction highlights every display of
that instruction and shows its index, step (if known), source ID, byte offset,
length and source SHA-256. The byte preview is limited to 64 bytes; API
`program.read_source(span)` returns the complete range. For GXW, offsets refer
to a logical stream, and `raw.sources[source_id]` identifies its container/path.
CSV offsets refer to the original UTF-16 file. They are not screen coordinates
or physical CFB-sector offsets.

`view._repr_svg_()` returns exactly the Rust-generated SVG, without an IPython
runtime dependency. Notebook rendering is static; interactive source selection
is in the standalone HTML. `view.elements` and the SVG `<metadata>` record map
each unique element ID to the original instruction/source. `view.graph` exposes
the condition DAG and ordered outputs/control instructions. All Python views
retain Rust source ownership after their input files and parent views disappear.

The layout is **reconstructed and rearranged**, with one tile per output
(`layout="output_conditions_relaid"`). It does not recover GX Works2 editor
coordinates. AND connections are series; OR connections are parallel; LDI/ANI/ORI
contacts are inverted; SET/RST coils carry S/R markers. The viewport scrolls for
large circuits, and the zoom slider changes the displayed size.

Each contact is a particular read instruction, identified by `#index`. When
MPS/MRD/MPP or a cascaded output reuses an evaluated condition, its contacts may
appear in multiple tiles with the **same index**. This represents reuse of that
result, not a second device read. A later LD of the same device has a different
index. In particular, OUT does not clear the current logic result: OUT followed
by LD/OR/ANB can reuse it. Do not evaluate a displayed tile by reading all its
devices again; the ordered source instructions remain the execution reference.

Reconstruction stops at the first instruction with unknown effects or a stack
error. Earlier resolved outputs remain visible with `complete=False`, a
**partial** banner, and the full original instruction list. Opaque ranges are
listed separately. Nothing after that point is silently converted into a
resolved circuit. END before the end of the list also leaves the result partial.

## Instruction diff

```python
other = gxwlib.load_csv("changed.csv", device_profile="fx")
changes = gxwlib.diff_programs(program, other)
for hunk in changes.hunks:
    print(hunk.kind, hunk.left_start, hunk.left_count,
          hunk.right_start, hunk.right_count)
    for span in hunk.left_sources:
        print(changes.left_program.read_source(span))
print(changes.complete, changes.different, changes.opaque_changed)
```

Hunks are `equal`, `insert`, `delete` or `replace`. Supported instructions compare
opcode and typed operands, ignoring spelling case, leading zeros, source spans,
step numbers and comments. Constant radix/width distinctions are retained.
Unknown instructions compare their original text and raw records, with overall
`complete=False`. Opaque bytes are compared separately via `opaque_changed`;
they have no instruction hunk and remain available in each input's
`opaque_regions`. Thus `different=False, complete=False` is not a complete
comparison. Even a complete, unchanged instruction sequence does not establish
execution equivalence of projects or their parameters.

## CLI and limits

```sh
gxw analyze project.gxw --device-profile fx --program 0 --external-write M100 --json
gxw render project.gxw --device-profile fx --format html --output ladder.html
gxw render MAIN.csv --input-format csv --device-profile fx --output ladder.svg
gxw diff old.gxw new.csv --right-format csv --device-profile fx --json
```

The input format defaults to GXW. CSV must be selected with `--input-format csv`;
`--right-format` defaults to the left format. `--program`/`--right-program` select
zero-based POU indices (CSV requires zero). Render defaults to SVG on stdout;
`--output` creates a file and `--force` permits replacing an existing output.
Overwriting the input itself is rejected.

Exit codes: 0 complete success/equal diff; 1 input/resource/output error;
2 command-line usage error; 3 partial result; 4 complete diff with changes.
A partial render still emits its annotated SVG/HTML.

`AnalysisOptions` defaults to 100,000 instructions, an 8-result block stack and
an 11-result MPS stack. Completed output results may roll out of the block stack
between independent rungs; unresolved block results cannot silently roll out.
These are selected FX limits, not detected project parameters. `render_ladder`
defaults to 20,000 expanded elements and 16 MiB per output artifact, with hard
configuration ceilings of 1,000,000 elements and 256 MiB. Shared DAG expansion is
bounded before drawing; long chains use iterative traversal. Labels in the SVG
and instruction preview are shortened to 100 characters. The complete source
remains in the IR. `diff_programs(max_cells=1_000_000)` bounds the LCS table after
trimming equal prefixes/suffixes, with a combined 200,000-instruction input cap.
Exceeding allocation/output limits raises `ResourceLimitError`, without silently
truncating a circuit or diff.

Rust callers use `analyze`, `build_ladder`, `render_svg` and `diff_programs`
with `Arc<InstructionProgram>` / `Arc<LadderGraph>`. The Rust example
`cargo run -p gxw-core --example analysis -- csv MAIN.csv` exports the report,
graph, SVG/HTML and source map together.
