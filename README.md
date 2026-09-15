# gxwlib

Read, analyze and display GX Works2 `.gxw` projects with a Rust core and a typed
Python API.

- Inspect project metadata and streams, with original source bytes and diagnostics.
- Decode a subset of simple-ladder instructions and GX Works2 list CSV exports.
- Find device references, compare instructions and display ladders as SVG or HTML.
- Inspect Structured Ladder/FBD blocks, connections, FB identities and declarations.
- Simulate one supported program using an explicit FX3G virtual scan model,
  including a limited timer/counter subset.

The package is read-only. Unsupported data remains inspectable with diagnostics.
Whole-project execution, structured POU execution and GX Simulator2 compatibility
are not supported. See the [documentation](docs/README.md) for API-specific limits.

## Install from source

Requires CPython 3.13 or later and Rust 1.93. From a source checkout:

```sh
python -m pip install .
```

Building from source requires Rust; using an installed wheel does not. The
Python package has no runtime package dependencies. Free-threaded Python and
PyPy are not supported.

## Inspect a project

```python
import gxwlib

project = gxwlib.load_project("project.gxw")
for index, program in enumerate(project.programs):
    print(index, program.logical_name, program.framing_status)

instructions = gxwlib.decode_program(project, 0, device_profile="fx")
report = gxwlib.analyze(instructions)
print(report.complete)
for finding in report.diagnostics:
    print(finding.code, finding.message)
```

For structured POUs, use `decode_structured(project, index)` and
`render_structured(program)`. Program indices refer to `project.programs`.

```sh
gxw inspect project.gxw --json
gxw analyze project.gxw --device-profile fx --json
gxw render project.gxw --device-profile fx --format html --output ladder.html
```

See [inspection](docs/inspection.md), [instruction decoding](docs/instructions.md),
[analysis and display](docs/analysis.md), [structured POUs](docs/structured.md), and
[simulation](docs/simulation.md). Rust entry points are documented alongside the
Python APIs; `gxw-core` is available in the Cargo workspace.

gxwlib is licensed under the [MIT License](LICENSE). Third-party components and
fixtures retain their own licenses; see [LICENSES](LICENSES/) and the
[fixture notices](tests/fixtures/README.md).
