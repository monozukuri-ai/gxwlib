# Structured Ladder/FBD and declarations

`decode_structured` reads the observed structured POU record profile in a current
CFB v3 project. It retains ordered blocks, nodes, ports, wires, FB instance/type
names, unknown records, and checked source spans. It does not compile or simulate
these graphs. ST, other envelopes and malformed boundaries are rejected.

```python
import gxwlib as gxw

raw = gxw.load_project("project.gxw")
program = gxw.decode_structured(raw, 0)  # index into raw.programs
for block in program.blocks:
    for node in block.nodes:
        print(block.index, node.kind, node.symbol, node.type_name, node.bounds)
        print(raw.read_source(node.source))
    for net in block.nets:
        print(net.ports, net.wires)

labels = gxw.decode_declarations(raw, 1)  # index into raw.index.logical_objects
for row in labels.rows:
    print(row.name, row.data_type, row.initial_value, row.type_reference)

view = gxw.render_structured(program)  # notebook SVG
html = view.to_html()  # offline selection of original node/port/wire bytes
```

`decode_ir(raw, index, device_profile="fx")` returns either `InstructionProgram`
or `StructuredProgram`. Rust exposes the corresponding `structured::ProgramIr`
enum. The explicit device profile applies to the instruction variant. A
structured POU is not converted into a sequential scan program. Existing
`analyze`, `render_ladder`, and `compile_program` accept only `InstructionProgram`.

## Structure and interpretation

- `structure_complete` means all records and flags belong to the supported
  structural profile. `semantic_status` remains `not_evaluated`, including for
  a graph made entirely of contacts and coils. Function/FB implementations are
  opaque; stored instance and type names are available with a diagnostic.
- `ordinal` refers to record order inside a block. Node and wire ordinals share
  this sequence. `net.ports` contains `(node_index, port_index)` pairs and
  `net.wires` contains wire indices, all scoped to that block.
- Coincident ports, ports on wires and wire endpoints on wires form geometric
  nets. Interior/interior crossings alone do not. Blocks never connect to one
  another based on coordinates. Direction, port type and native evaluation
  order are not inferred from numeric `kind_code` values.
- Unknown record classes/kinds are retained as opaque spans. Unknown node/wire
  flags or diagonal wires mark the block incomplete and suppress its nets.
  Invalid sizes, counts, UTF-16 or overflowing coordinates raise an exception.
- Declaration scope is selected by `.Labels.lh` or `.gh`. Rows retain all saved
  fields, including array markers and type references. Initial values, arrays,
  unknown integer fields and FB types are not evaluated. No automatic symbol
  binding or FB body resolution is performed. Duplicate names/IDs and unknown
  trailers produce diagnostics. Header/trailer spans preserve opaque bytes.
- Spans are logical stream offsets, paired with the project SHA-256, never file
  offsets. Frozen Python child views keep their source owner alive. Passing a
  span to a different project raises `ValueError`.

The viewer stacks stored block coordinates vertically and draws ports, wires
and simplified node symbols. It is a structural view, not a GX Works2 pixel
replica or power-flow display. Unknown records are listed visibly. SVG/HTML
labels are escaped; selecting an element shows its source span and up to 64 raw
bytes. Block order does not establish task or scan order.

## Bounds and CLI

`StructuredLimits` defaults: 4,096 blocks, 100,000 records, 200,000 ports, 65,536
UTF-16 units per string, 65,536 declaration rows, and 5,000,000 pair checks for
connectivity across the entire POU. Counts are checked before allocation and
quadratic work. File/stream limits remain controlled by `ReadLimits` at load.
Rendering defaults to 20,000 selectable elements and 16 MiB output; excessively
large view coordinates are rejected separately. Resource limits raise
`ResourceLimitError` rather than silently truncating the graph.

```sh
gxw structured project.gxw --program-index 0 --json
gxw declarations project.gxw --logical-index 1 --json
gxw render-structured project.gxw --format html --output view.html
cargo run -p gxw-core --example structured -- pou project.gxw 0
```

CLI exit status: 0 for complete structural decoding, 3 for retained partial
structure, 1 for errors. A successful exit does not imply executable semantics.
`render-structured` refuses to overwrite the input, including aliases/hard links.

## CPU profiles

`parse_device_address(text, family="fx" | "q" | "l")` recognizes literal
X/Y/M/D/T/C spelling independently from instruction decoding or execution. For
example, `X10` is address 8 for FX and address 16 for Q/L. Q/L `X1F` is 31.
The Q/L radix table comes from Mitsubishi's
[Structured Programming Manual, SH-080785ENG-M](https://dl.mitsubishielectric.com/dl/fa/document/manual/plc/sh080785eng/sh080785engm.pdf).

Every returned `DeviceAddress` has `range_status="not_checked"`. CPU-specific
ranges, indexed devices, instructions, timer bases/retention and task order are
outside this spelling API. The existing `DeviceProfile::Fx` instruction parser
and FX3G simulator remain separate; `q`/`l` cannot be passed to them. Q/L
instruction decoding and execution are unsupported.
The project CPU is not inferred from an address or POU geometry.

Binary record/declaration layouts and geometric connectivity were adapted from
[gxworks-agent](https://github.com/Arienax/gxworks-agent/tree/d758bc3106b711e4a2090e95bda119c3a2af06de),
under Apache-2.0; the license and modification notice ship in
`LICENSES/gxworks-agent.txt`.
