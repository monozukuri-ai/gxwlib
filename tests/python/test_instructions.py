import csv
import gc
import io
import json
import subprocess
import sys
from hashlib import sha256
from pathlib import Path

import pytest

import gxwlib

FIXTURES = Path(__file__).resolve().parents[1] / "fixtures"
PAIR = FIXTURES / "third_party" / "ladder_converter"


def csv_data(instructions):
    output = io.StringIO(newline="")
    writer = csv.writer(output, delimiter="\t", quoting=csv.QUOTE_ALL, lineterminator="\r\n")
    writer.writerows(
        [
            ["自作😀"],
            ["PC情報:", "FXCPU FX3G"],
            [
                "ステップ番号",
                "行間ステートメント",
                "命令",
                "I/O(デバイス)",
                "空欄",
                "PIステートメント",
                "ノート",
            ],
        ]
    )
    for step, mnemonic, operand in instructions:
        writer.writerow([step, "", mnemonic, operand, "", "", ""])
    return b"\xff\xfe" + output.getvalue().encode("utf-16-le")


def test_vendor_pair_and_independent_standard_library_csv_reader():
    metadata = json.loads((PAIR / "provenance.json").read_text())
    for name, digest in metadata["sha256"].items():
        assert sha256((PAIR / name).read_bytes()).hexdigest() == digest
    rows = list(
        csv.reader(io.StringIO((PAIR / "MAIN.csv").read_bytes().decode("utf-16")), delimiter="\t")
    )
    expected = []
    for row in rows[3:]:
        if row[2]:
            expected.append([int(row[0]), row[2], []])
        if row[3]:
            expected[-1][2].append(row[3])
    csv_program = gxwlib.load_csv(PAIR / "MAIN.csv", device_profile="fx")
    assert [r.fields for r in csv_program.csv_rows] == rows
    assert [
        [i.step, i.original_mnemonic, [o.original_text for o in i.operands]]
        for i in csv_program.instructions
    ] == expected
    raw = gxwlib.load_project(PAIR / "FX2N.gxw")
    program = gxwlib.decode_program(raw, 0, device_profile="fx")
    assert len(program.instructions) == 170
    assert sum(i.supported for i in program.instructions) == 83
    assert not program.complete and not csv_program.complete
    assert program.opaque_regions == []
    assert {i.opcode for i in program.instructions if i.supported} == {
        "LD",
        "LDI",
        "AND",
        "ANI",
        "OR",
        "ORI",
        "OUT",
        "SET",
        "RST",
        "ANB",
        "ORB",
        "MPS",
        "MRD",
        "MPP",
        "END",
    }
    for ins, (_, mnemonic, operands) in zip(program.instructions, expected, strict=True):
        if ins.mnemonic is not None:
            assert ins.mnemonic == mnemonic
        if ins.supported:
            assert len(ins.operands) == len(operands)
            for operand, spelling in zip(ins.operands, operands, strict=True):
                assert operand.value.device == spelling[0]
                assert operand.value.address == int(spelling[1:], 8 if spelling[0] in "XY" else 10)
    constants = [
        o.value for i in program.instructions for o in i.operands if o.value.kind == "constant"
    ]
    assert {o.bit_width for o in constants} == {16, 32}
    assert any(o.radix == 16 and o.value == 0x1234 for o in constants)


def test_immutable_views_own_sources_after_parents_and_file_are_removed(tmp_path):
    path = tmp_path / "日本語.csv"
    data = csv_data([(0, "ld", "X010"), (1, "OUT", "Y1"), (2, "END", "")])
    path.write_bytes(data)
    program = gxwlib.load_csv(path, device_profile="fx")
    path.unlink()
    instruction = program.instructions[0]
    operand = instruction.operands[0]
    span = operand.source
    assert program.read_source(span) == '"X010"'.encode("utf-16-le")
    other = gxwlib.parse_csv_bytes(data, device_profile="fx")
    with pytest.raises(ValueError, match="another"):
        other.read_source(span)
    del program, instruction
    gc.collect()
    assert operand.original_text == "X010" and operand.value.address == 8
    with pytest.raises(AttributeError):
        operand.original_text = "X0"
    raw = gxwlib.load_project(PAIR / "FX2N.gxw")
    decoded = gxwlib.decode_program(raw, 0, device_profile="fx")
    source = decoded.instructions[0].source
    del raw
    gc.collect()
    assert decoded.read_source(source) == bytes.fromhex("03 00 03 04 9c 00 04")


def test_negative_constants_unknown_width_and_unsupported_forms():
    program = gxwlib.parse_csv_bytes(
        csv_data([(0, "MOV", "K-2147483648"), ("", "", "D0"), (5, "END", "")]), device_profile="fx"
    )
    assert not program.complete
    assert program.instructions[0].operands[0].value.value == -2147483648
    assert program.instructions[0].operands[0].value.bit_width is None
    for device in ["X008", "M-1", "K2147483648", "D100Z0"]:
        program = gxwlib.parse_csv_bytes(
            csv_data([(0, "LD", device), (1, "END", "")]), device_profile="fx"
        )
        assert not program.complete
        assert program.instructions[0].operands[0].value.kind == "unknown"


def test_explicit_profile_errors_and_resource_bounds():
    data = csv_data([(0, "END", "")])
    with pytest.raises(TypeError):
        gxwlib.parse_csv_bytes(data)
    with pytest.raises(ValueError, match="device_profile"):
        gxwlib.parse_csv_bytes(data, device_profile="q")
    with pytest.raises(gxwlib.FormatError):
        gxwlib.parse_csv_bytes(data[:-1], device_profile="fx")
    with pytest.raises(gxwlib.ResourceLimitError):
        gxwlib.parse_csv_bytes(data, device_profile="fx", limits=gxwlib.ReadLimits(max_tokens=0))
    with pytest.raises(gxwlib.ResourceLimitError):
        gxwlib.parse_csv_bytes(
            data, device_profile="fx", limits=gxwlib.ReadLimits(max_file_bytes=2)
        )
    with pytest.raises(gxwlib.FormatError):
        gxwlib.decode_program(gxwlib.load_project(FIXTURES / "raw.gxw"), 1, device_profile="fx")
    with pytest.raises(TypeError):
        gxwlib.load_csv(b"test.csv", device_profile="fx")


def test_instruction_cli_status_and_json(tmp_path):
    path = tmp_path / "日本語.csv"
    path.write_bytes(
        csv_data(
            [
                (0, "LD", "X010"),
                (1, "OUT", "Y0"),
                (2, "AND", "M1"),
                (3, "OUT", "Y1"),
                (4, "OUT", "Y2"),
                (5, "END", ""),
            ]
        )
    )
    cases = [
        ("csv", path, 0),
        ("csv", PAIR / "MAIN.csv", 3),
        ("decode", PAIR / "FX2N.gxw", 3),
        ("decode", FIXTURES / "raw.gxw", 3),
        ("decode", FIXTURES / "minimal.gxw", 3),
    ]
    for command, file, status in cases:
        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "gxwlib",
                command,
                str(file),
                "--device-profile",
                "fx",
                "--json",
            ],
            capture_output=True,
            check=False,
            encoding="utf-8",
        )
        assert result.returncode == status, result.stderr
        assert isinstance(json.loads(result.stdout), list)
    for command in ["csv", "decode"]:
        result = subprocess.run(
            [sys.executable, "-m", "gxwlib", command, str(path)], capture_output=True, check=False
        )
        assert result.returncode == 2
    path.write_bytes(b"invalid")
    result = subprocess.run(
        [sys.executable, "-m", "gxwlib", "csv", str(path), "--device-profile", "fx"],
        capture_output=True,
        check=False,
    )
    assert result.returncode == 1
