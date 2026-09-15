"""Run outside the checkout in a clean environment containing the installed wheel."""

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from hashlib import sha256
from pathlib import Path

import gxwlib
from gxwlib import _core


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--raw-fixture", type=Path)
    parser.add_argument("--structured-fixture", type=Path)
    args = parser.parse_args()
    extension = Path(_core.__file__).resolve()
    assert extension.is_relative_to(Path(sys.prefix).resolve()), extension
    assert shutil.which("cargo") is None and shutil.which("rustc") is None
    data = args.fixture.read_bytes()
    with tempfile.TemporaryDirectory() as temp:
        path = Path(temp) / "日本語 project.gxw"
        path.write_bytes(data)
        index = gxwlib.inspect(path)
        assert index.sha256 == sha256(data).hexdigest()
        assert index.logical_objects[0].logical_name == "MAIN.プログラム.pou"
        cli = shutil.which("gxw")
        assert cli is not None
        result = subprocess.run(
            [cli, "inspect", str(path), "--json"], check=True, capture_output=True, encoding="utf-8"
        )
        assert json.loads(result.stdout) == json.loads(index.to_json())
        assert path.read_bytes() == data
        csv_text = (
            '"synthetic"\r\n"PC情報:"\t"FXCPU FX3G"\r\n'
            '"ステップ番号"\t"行間ステートメント"\t"命令"\t"I/O(デバイス)"\t"空欄"\t"PIステートメント"\t"ノート"\r\n'
            '"0"\t""\t"END"\t""\t""\t""\t""\r\n'
        )
        csv_path = Path(temp) / "日本語.csv"
        csv_path.write_bytes(b"\xff\xfe" + csv_text.encode("utf-16-le"))
        instructions = gxwlib.load_csv(csv_path, device_profile="fx")
        assert instructions.complete and instructions.instructions[0].opcode == "END"
        result = subprocess.run(
            [cli, "csv", str(csv_path), "--device-profile", "fx", "--json"],
            check=True,
            capture_output=True,
            encoding="utf-8",
        )
        assert json.loads(result.stdout) == [json.loads(instructions.to_json())]
        report = gxwlib.analyze(instructions)
        assert report.complete
        doc = gxwlib.render_ladder(instructions)
        assert doc.complete and doc._repr_svg_() == doc.svg
        assert "<svg " in doc.to_html()
        assert not gxwlib.diff_programs(instructions, instructions).different
        for command in ["analyze", "render", "diff"]:
            arguments = [
                cli,
                command,
                str(csv_path),
                "--device-profile",
                "fx",
                "--input-format",
                "csv",
            ]
            if command == "diff":
                arguments.append(str(csv_path))
            if command != "render":
                arguments.append("--json")
            result = subprocess.run(arguments, check=True, capture_output=True, encoding="utf-8")
            if command == "analyze":
                assert json.loads(result.stdout) == json.loads(report.to_json())
            elif command == "render":
                assert result.stdout == doc.svg
            else:
                assert json.loads(result.stdout)["different"] is False
        # Exercise actual state changes from the installed extension and CLI.
        simulation_csv = csv_text.replace('"0"\t""\t"END"', '"2"\t""\t"END"')
        header, end = simulation_csv.rsplit('"2"', 1)
        simulation_csv = (
            header
            + '"0"\t""\t"LD"\t"X0"\t""\t""\t""\r\n"1"\t""\t"OUT"\t"Y0"\t""\t""\t""\r\n"2"'
            + end
        )
        csv_path.write_bytes(b"\xff\xfe" + simulation_csv.encode("utf-16-le"))
        p = gxwlib.load_csv(csv_path, device_profile="fx")
        compiled = gxwlib.compile_program(p, profile="fx3g")
        simulator = gxwlib.Simulator(compiled, scan_period_ns=17)
        simulator.set_inputs({"X0": True})
        trace = simulator.run(3, watch=["Y0"])
        assert trace.final_snapshot.output("Y0") and trace.final_snapshot.time_ns == 51
        result = subprocess.run(
            [
                cli,
                "simulate",
                str(csv_path),
                "--input-format",
                "csv",
                "--profile",
                "fx3g",
                "--scan-period-ns",
                "17",
                "--scans",
                "3",
                "--input",
                "X0=1",
                "--watch",
                "Y0",
            ],
            check=True,
            capture_output=True,
            encoding="utf-8",
        )
        assert json.loads(result.stdout) == json.loads(trace.to_json())
        # Timer/counter execution from an installed extension without Rust on PATH.
        timer_csv = simulation_csv.replace('"OUT"\t"Y0"', '"OUT"\t"T200"').replace(
            '"2"\t""\t"END"',
            '""\t""\t""\t"K2"\t""\t""\t""\r\n'
            '"2"\t""\t"OUT"\t"C16"\t""\t""\t""\r\n'
            '""\t""\t""\t"K1"\t""\t""\t""\r\n'
            '"3"\t""\t"END"',
        )
        csv_path.write_bytes(b"\xff\xfe" + timer_csv.encode("utf-16-le"))
        p = gxwlib.load_csv(csv_path, device_profile="fx")
        sim = gxwlib.Simulator(gxwlib.compile_program(p, profile="fx3g"), scan_period_ns=10_000_000)
        sim.set_inputs({"X0": True})
        timing = sim.run(3, watch=["T200", "C16"])
        assert [s.timers["T200"].value for s in timing.samples] == [0, 1, 2]
        assert [s.counters["C16"].value for s in timing.samples] == [1, 1, 1]
        sim.stop()
        sim.advance_stopped(1_000_000_000)
        sim.start()
        assert sim.step().counter("C16").value == 1
        result = subprocess.run(
            [
                cli,
                "simulate",
                str(csv_path),
                "--input-format",
                "csv",
                "--profile",
                "fx3g",
                "--scan-period-ns",
                "10000000",
                "--input",
                "X0=1",
                "--scans",
                "3",
                "--watch",
                "T200",
                "--watch",
                "C16",
            ],
            check=True,
            capture_output=True,
            encoding="utf-8",
        )
        assert json.loads(result.stdout) == json.loads(timing.to_json())
        if args.raw_fixture:
            raw = gxwlib.load_project(args.raw_fixture)
            assert raw.framing_complete and raw.semantic_status == "not_decoded"
            assert raw.programs[0].tokens[0].data_hex == "03 ee 03"
            result = subprocess.run(
                [cli, "parse", str(args.raw_fixture), "--json"],
                check=True,
                capture_output=True,
                encoding="utf-8",
            )
            assert json.loads(result.stdout) == json.loads(raw.to_json())
            decoded = gxwlib.decode_program(raw, 0, device_profile="fx")
            assert not decoded.complete and decoded.opaque_regions
            result = subprocess.run(
                [cli, "decode", str(args.raw_fixture), "--device-profile", "fx", "--json"],
                check=False,
                capture_output=True,
                encoding="utf-8",
            )
            assert result.returncode == 3
            assert json.loads(result.stdout) == [json.loads(decoded.to_json())]
        if args.structured_fixture:
            project = gxwlib.load_project(args.structured_fixture)
            structured = gxwlib.decode_structured(project, 0)
            assert (
                structured.structure_complete and structured.blocks[0].nodes[1].symbol == "積算器"
            )
            assert gxwlib.decode_declarations(project, 1).rows[0].type_reference == "MY_FB"
            assert "<svg " in gxwlib.render_structured(structured).to_html()
            assert gxwlib.parse_device_address("X1F", family="q").address == 31
            result = subprocess.run(
                [cli, "structured", str(args.structured_fixture), "--json"],
                check=True,
                capture_output=True,
                encoding="utf-8",
            )
            assert json.loads(result.stdout) == json.loads(structured.to_json())
    print(
        json.dumps(
            {
                "version": gxwlib.__version__,
                "extension": str(extension),
                "python": sys.version,
                "installed_smoke": "passed",
                "rust_toolchain_on_path": False,
            }
        )
    )


if __name__ == "__main__":
    main()
