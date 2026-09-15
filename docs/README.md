# Documentation

Start with the [installation and quickstart](../README.md).

| Task | Guide |
|---|---|
| List metadata and stored streams | [Inspection](inspection.md) |
| Read raw POU records and original bytes | [Raw parsing](raw-parsing.md) |
| Decode instructions or import GX Works2 list CSV | [Instruction decoding](instructions.md) |
| Find device references, compare programs, display ladders | [Analysis and display](analysis.md) |
| Inspect FBD/structured ladder geometry and label declarations | [Structured POUs](structured.md) |
| Run an isolated FX3G virtual scan | [Simulation](simulation.md) |
| Configure timers, counters, virtual time and STOP/RUN | [Timing](timing.md) |

Each API reports its own support boundary. Container inspection, instruction
decoding, graph reconstruction and simulation are separate operations; success
at one level does not establish support at the next. Check the returned status
and diagnostics before using a partial result.

Examples using `project.gxw`, `MAIN.csv` or `timers.csv` expect your own input
files. Select program and declaration indices from the inspected project.
The package never writes changes back to a GXW project or connects to a PLC.
