# Repository validation tools

These scripts support reproducible checks of a source checkout or source
distribution. They use the bundled fixtures and optional caller-supplied files.
They are not installed as commands in the Python wheel; use `gxw --help` for the
package CLI.

| Scripts | Purpose |
|---|---|
| `check_binding_parity.py`, `check_instruction_parity.py` | Compare container, record and instruction APIs with Rust |
| `check_analysis_parity.py` | Compare analysis, SVG/HTML and program diff |
| `check_simulation_parity.py`, `check_timing_parity.py` | Compare virtual scans and timer/counter state |
| `check_structured_parity.py` | Compare structured POU/declaration APIs and CLI |
| `create_ladder_review.py`, `create_structured_review.py` | Generate browser cases in a selected output directory |
| `check_ladder_browser.mjs`, `check_structured_browser.mjs` | Check display and source selection with Chrome and Playwright Core |
| `check_distribution.py` | Check archive contents, versions, licenses, wheel RECORD and private-path Git exclusions |
| `check_release.py` | Check the release tag, locked versions, four-platform wheel set and SHA256SUMS |
| `check_cold_install.py`, `smoke_installed.py` | Install a wheel into a fresh environment and exercise the package |
| `check_sdist_build.py` | Rebuild and test a wheel from a source distribution |
| `collect_licenses.py` | Refresh notices for dependencies in Cargo.lock |
| `check_public_docs.py` | Check public documentation links and packaging boundaries |

Run Python tools with the project environment and use `--help` for arguments.
Parity tools require the corresponding compiled Rust example. Browser tools
take a case manifest, Chrome executable and installed Playwright Core module
path; those tools are not runtime dependencies of gxwlib.

License collection reads the Cargo registry cache and updates `LICENSES/`.
Installation checks create temporary environments and install validation
dependencies. The resulting wheel is exercised without Rust tools on PATH.
These checks do not publish packages or establish PLC execution compatibility.
