# Inspection API

`gxwlib.inspect(path, *, limits=None)` accepts a string or `os.PathLike[str]`.
`gxwlib.inspect_bytes(data, *, limits=None)` accepts immutable `bytes` and copies
them into Rust-owned memory before detaching from Python. Both return a
`ProjectIndex`. The Rust equivalents are `inspect_path` and `inspect_bytes`.

An index contains:

- Input byte length, SHA-256 and JSON `schema_version` (currently 1).
- Two container inventories: `path=[]` for the outer CFB and `path=["_hdb"]`
  for its nested CFB. Each stream has a component path, byte length and SHA-256.
- Logical objects from `projectdatalist.xml`, including original row fields,
  qualified attribute names, ancestor names, activity flags and resolved locations.
- Rows from `projectlist.xml`, when present, and diagnostics.

Paths inside CFB are arrays of components, independent of the operating system.
A location contains separate `container` and `path` arrays. It does not represent
a filesystem path or a byte offset in the outer file. Inventories are sorted by
component path; metadata row order is preserved.

The inspect API uses `cfb` strict validation without retrying in permissive mode. It
reads every stream to hash its payload, retains only the XML and `_hdb` required
for inspection, and returns an index owning its strings. Arbitrary embedded
streams are not recursively decoded. Extensions such as `user.xml` do not cause
binary streams to be parsed as XML.

UTF-8 and UTF-16LE/BE metadata are decoded strictly. DTDs, unknown entities,
invalid encodings and nested metadata field structures are rejected. This is a
bounded row reader, not full GX Works2 metadata/schema support. Namespace
declarations and attributes of the source XML remain recoverable through the
original stream and its hash; the index does not expose a complete XML syntax tree.

Only rows with an explicit false scrap flag and outside `before`/`errors`
ancestors in the DiffGram namespace are resolved. Foreign metadata namespaces or
parents are diagnosed without resolving them. A numbered stream in `_hdb` or an outer stream matching
the logical name is accepted only when the match is unambiguous. Repeated IDs,
unknown activity flags and absent streams remain unresolved with diagnostics.
CPU, security configuration, task order and instruction meanings are not inferred.

Python project/container/stream/logical-object views share Rust ownership. They
remain valid after deleting the parent variable or original file. Properties are
read-only; returned lists and dictionaries are independent containers.
`to_json()` explicitly exports the whole index. Parsing and JSON export release
the GIL; independent inspections can run concurrently.

## Errors and limits

Filesystem errors become `OSError` subclasses (including `FileNotFoundError`).
`FormatError`, `UnsupportedFeatureError` and `ResourceLimitError` inherit from
`GxwError`. The CLI returns 1 on these errors and 2 for invalid arguments.

Default `ReadLimits`:

| Parameter | Default |
|---|---:|
| `max_file_bytes` | 64 MiB |
| `max_stream_bytes` | 64 MiB |
| `max_total_stream_bytes` | 256 MiB |
| `max_entries` | 65,536 across both CFBs, including storages |
| `max_xml_bytes` | 8 MiB per metadata stream |
| `max_xml_depth` | 64 |
| `max_xml_rows` | 65,536 per metadata stream |

These bounds do not constitute an exact process memory limit: CFB allocation
tables, decoded strings and metadata objects also consume memory. Limits apply
before retaining oversized stream payloads; input growth is checked during reads.

Every successful index has `GXW_INSPECT_ONLY`. Other current diagnostic codes
are `GXW_PROJECTLIST_MISSING`, `GXW_ACTIVITY_UNKNOWN`,
`GXW_LOGICAL_FIELDS_MISSING`, `GXW_DUPLICATE_LOGICAL_ID`,
`GXW_AMBIGUOUS_LOCATION`, `GXW_METADATA_SCHEMA_UNSUPPORTED` and `GXW_UNRESOLVED_LOGICAL_OBJECT`.
Successful inspection does not imply instruction, ladder or execution compatibility.

See [raw POU parsing](raw-parsing.md) for the separate `load_project` API and its
additional project-wide `max_tokens` bound.
