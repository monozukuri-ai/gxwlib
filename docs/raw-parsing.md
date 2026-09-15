# Raw POU parsing

`gxwlib.load_project(path, *, limits=None)` and `gxwlib.loads(bytes, *, limits=None)`
return a `ParsedProject`. Rust entry points are `load_path(&Path, &ReadLimits)`
and `parse_bytes(Arc<[u8]>, &ReadLimits)`.

`inspect` remains the container/metadata API. Loading additionally retains the
outer and `_hdb` stream bytes and frames current POU candidates. It does not
decode instructions, select a CPU execution profile or implement simulation.

```python
import gxwlib

project = gxwlib.load_project("project.gxw")
for program in project.programs:
    print(program.logical_name, program.profile, program.framing_status)
    for token in program.tokens:
        print(token.source.offset, token.source.length, token.data_hex)
    for diagnostic in program.diagnostics:
        print(diagnostic.code, diagnostic.source)
```

The CLI equivalent is `gxw parse file.gxw --json`. Exit codes are 0 when all
selected POUs have complete framing, 3 for partial/unsupported framing or no
current POU candidates, 1 for file/container/XML/resource errors, and 2 for CLI
usage errors. A successful exit proves framing only.

## Current profile

`simple-le-observed-v1` and `simple-le-observed-v2-trailer24` support specific
simple-ladder record layouts. They do not cover every GX Works2 project format.

Candidates are discovered by `.pou` names or the metadata pair `ucFolderType=7`,
`ucFileType=2`. Profile selection requires that metadata pair, an unambiguous
current stream, CFB version 3 in both containers, and the observed header families.
Names such as `MAIN.Program.pou`, `MAIN.プログラム.pou`, numeric stream IDs, and
CPU literal strings are not used as hardcoded profile selectors. Renaming a POU
does not change its framing when the metadata kind and payload remain valid.

All token contents remain raw, including byte sequences whose instruction
meaning is unknown. The separate [instruction decoder](instructions.md) adds
an IR without changing the raw result.

Framing validates the header, declared lengths, token boundaries, trailer and
terminal record. `simple-le-observed-v2-trailer24` handles a distinct header and
trailer layout; arbitrary padding is not accepted. Structured POUs use the
separate [structured decoder](structured.md), even when raw framing reports
`unsupported`.

- `complete`: the selected profile's framing checks succeeded.
- `partial`: header family matched, but a length, trailer, token boundary or
  terminal-record check failed.
- `unsupported`: profile/metadata could not be selected, or the logical source
  could not be resolved.

If a token boundary is invalid, valid preceding tokens remain available and the
rest of the token region is opaque. The parser does not scan ahead to guess a
new boundary. Invalid length/trailer relationships leave the full POU opaque.
Unsupported streams are retained without frame interpretation. Every project
reports `semantic_status="not_decoded"`, `cpu_model=None`, `execution_order=None`.

## Source and ownership

`project.index` is the original inventory contract. `project.sources` assigns IDs
to retained logical streams and records each location, size and SHA-256. Pair a
source ID with `project.index.sha256`: IDs are scoped to a particular project.
Each `SourceInfo.source` is the span for that entire logical stream, so
`project.read_source(info.source)` also retrieves opaque metadata/binary streams.

`SourceSpan` contains `source_id`, `offset`, `length`. Offsets are bytes in that
logical stream, never physical offsets in the outer CFB file. Programs expose
their full source, metadata-row source, token region, trailer, and opaque regions.
`project.read_source(span)` returns the original bytes for a checked span; Python
rejects spans belonging to another loaded project. Token `data` and `data_hex`
are explicit byte/hex copies on access. Rust tokens retain ranges, not individual
copies of the token payload.

POU metadata-row spans refer to the original XML bytes even for UTF-16, BOMs,
surrogate pairs and entity references. The XML reader resolves namespace URIs
for history detection. Foreign metadata namespaces/parents remain unresolved;
current rows, scrap rows, duplicate IDs, duplicate names and history are not
collapsed into one program. Original XML stream bytes retain namespace declarations
and attributes not interpreted by the bounded row reader.

Python views keep shared Rust ownership and remain usable after dropping their
parents or deleting the input file. Parsing and JSON export detach from Python.
`ReadLimits.max_tokens` defaults to 1,000,000 across all selected POUs. Existing
file, stream, aggregate stream and XML bounds still apply. Source buffers count
both `_hdb` itself and its nested streams; the limits are not exact RSS bounds.

The inventory JSON and raw-project JSON have separate schema version 1 contracts.
Raw JSON includes token hex and source spans; it does not embed all retained streams.

## Diagnostics

| Code | Meaning |
|---|---|
| `GXW_RAW_ONLY` | Raw framing only; no instruction or execution semantics |
| `GXW_METADATA_SCHEMA_UNSUPPORTED` | Metadata namespace/parent is not the observed schema |
| `GXW_POU_UNRESOLVED` | No unambiguous current source |
| `GXW_POU_METADATA_UNSUPPORTED` | Candidate metadata or CFB version is not supported |
| `GXW_POU_PROFILE_UNSUPPORTED` | POU header is not a recognized family |
| `GXW_POU_LENGTH_MISMATCH` | Two length fields disagree |
| `GXW_POU_LENGTH_INVALID` | Declared length is shorter than the trailer |
| `GXW_POU_SIZE_MISMATCH` | Declared length and stream length differ |
| `GXW_POU_TRAILER_UNSUPPORTED` | Trailer does not match the observed layout |
| `GXW_TOKEN_FRAME_INVALID` | Next token boundary cannot be established |
| `GXW_POU_TERMINATOR_MISSING` | Observed terminal record is absent |

File/CFB/XML corruption and resource limits remain exceptions. POU framing
failures remain inspectable results with diagnostics, without executable code.
