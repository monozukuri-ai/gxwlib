# Instruction decoding and GX Works2 CSV

The Rust core provides `decode_program(Arc<ParsedProject>, index, DeviceProfile::Fx)`,
`load_csv(path, profile, limits)` and `parse_csv_bytes(Arc<[u8]>, profile, limits)`.
Python exposes the same operations through immutable Rust-backed views:

```python
import gxwlib

raw = gxwlib.load_project("project.gxw")
program = gxwlib.decode_program(raw, 0, device_profile="fx")
exported = gxwlib.load_csv("MAIN.csv", device_profile="fx")

for instruction in program.instructions:
    print(instruction.ordinal, instruction.opcode, instruction.supported)
    for operand in instruction.operands:
        print(operand.value.kind, operand.value.device, operand.value.address)
    print(program.read_source(instruction.source).hex(" "))
```

`program_index` indexes `ParsedProject.programs`, not metadata rows. The caller
must select FX device syntax explicitly. This does not decode or confirm CPU
model, device ranges, program execution order, timer units or counter widths.
The raw APIs retain their existing `semantic_status="not_decoded"` contract.

## Supported syntax and incomplete results

`Opcode` covers LD, LDI, AND, ANI, OR, ORI, OUT, SET, RST, ANB, ORB,
MPS, MRD, MPP and END. Instructions remain in source order, including contacts
following outputs and consecutive outputs. No rung grouping is inferred.

`Instruction.supported` checks this opcode set, operand forms and arity.
`InstructionProgram.complete` additionally requires complete framing, one final
END, no opaque regions and no diagnostics. It is a **syntax result**, not a
claim that the program is executable, logically valid or simulator-compatible.
Stack underflow is reported by [static analysis](analysis.md) and rejected by
[simulation compilation](simulation.md).

Recognized forms outside the supported opcode set retain a known mnemonic and
operand boundaries, but have `opcode=None` and `supported=False`. Pointer labels
remain unnamed unsupported instructions. An unrecognized record boundary stops
decoding and retains the entire remaining token region. Unknown instructions
are never substituted with NOP.

| Operand | CSV | Binary subset |
|---|---|---|
| X/Y | Octal unsigned address | One-byte compact address |
| M | Decimal unsigned address | One/two-byte compact address |
| D | Decimal unsigned address | One-byte compact address |
| T/C | Decimal address; contact/reset and OUT with preset | Unverified, retained as unknown |
| K | Signed 32-bit decimal value; width remains unknown | Compact positive 0–127; separate observed 16/32-bit tags |
| H | Unsigned hexadecimal bit pattern up to 32 bits | Observed two-byte, 16-bit form |
| Labels, indexed/grouped operands | Original text retained, unknown value | Observed modifiers retained with their operand, unknown value |

Address ranges in this table describe encoding/syntax bounds, not a particular
CPU's valid devices. Negative binary constants, additional address widths and
T/C encodings are unsupported. `encoding_width_bits` describes the compact payload;
it is separate from a constant's `bit_width`. CSV does not infer a bit width from
the number of digits. `HFFFFFFFF` retains unsigned bits, without choosing signed
interpretation. Operand and instruction original spellings remain available;
ASCII case variants normalize to uppercase. Unverified mnemonic aliases remain
unsupported instead of being mapped by guesswork.

## CSV format and source ranges

The parser follows the list-export format in the
[GX Works2 Simple Project manual, §6.16](https://www.mitsubishielectric.com/dl/fa/document/manual/plc/sh080780eng/sh080780engaf.pdf):
UTF-16LE with BOM, quoted tab-separated fields, doubled quotes and CRLF record
separators. A final record without CRLF is accepted. Quoted fields may contain
tabs and line breaks. Invalid UTF-16, NUL, malformed quoting and orphan operand
continuations are errors. The first three rows are project, PC information and
the seven-column header. Japanese headers and English
`Step No.`/`Instruction` headers are recognized.
Other dialects, UTF-8 exports and GX Converter files with blank metadata rows
are currently unsupported.

`csv_rows` retains all metadata, comments, notes and operand continuation rows.
CSV `step` numbers are preserved and, when present, must strictly increase.
They are not byte offsets or instruction ordinals. Binary steps remain `None`.
Comments are retained without interpreting their statement semantics.

`InstructionSpan` uses original byte offsets: logical stream bytes for GXW
(`source_id` set), original file bytes including the BOM for CSV (`source_id=None`).
Instruction spans include their operands; CSV operand spans include the quoted
field. Field and row spans remain correct for surrogate pairs and quoted newlines.
Each span belongs to one `InstructionProgram`, identified by `source_sha256`.
Python views keep the source alive after files and parent variables are removed;
`read_source` rejects spans from another program and returns a bytes copy.

CSV uses `ReadLimits.max_file_bytes`, `max_entries` for rows, and `max_tokens`
for instructions, with at most seven columns and seven operands per instruction.
GXW decoding uses the already bounded raw token collection.

## CLI

```sh
gxw decode project.gxw --device-profile fx --json
gxw csv MAIN.csv --device-profile fx --json
```

Both JSON commands return a list of instruction programs. Exit codes: 0 for
complete syntax, 3 for incomplete/unsupported/no programs, 1 for input errors,
2 for argument errors. JSON schema version is 1, independently of raw/index JSON.

FX device notation and numeric representation are based on the
[FX programming manual, chapters 4–5](https://www.mitsubishielectric.com/dl/fa/document/manual/plc_fx/jy997d16601/jy997d16601r.pdf).
