import http.client
import os
import queue
import signal
import subprocess
import sys
import threading
from contextlib import contextmanager
from pathlib import Path
from urllib.parse import urlsplit

import pytest

import gxwlib
from gxwlib import viewer

FIXTURES = Path(__file__).resolve().parents[1] / "fixtures"
CSV = FIXTURES / "ladder" / "or_blocks.csv"


@contextmanager
def running_viewer(*args):
    process = subprocess.Popen(
        [sys.executable, "-m", "gxwlib", *map(str, args), "--no-browser"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        encoding="utf-8",
    )
    lines = queue.Queue()
    threading.Thread(target=lambda: lines.put(process.stdout.readline()), daemon=True).start()
    try:
        url = lines.get(timeout=15).strip()
        if not url:
            raise AssertionError(process.communicate(timeout=5))
        address = urlsplit(url)
        assert address.scheme == "http" and address.hostname == "127.0.0.1"
        assert address.port and address.path == "/"
        yield process, address
    finally:
        if process.poll() is None:
            process.terminate()
        try:
            process.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.communicate(timeout=5)


def request(address, method="GET", path="/", headers=None):
    connection = http.client.HTTPConnection(address.hostname, address.port, timeout=5)
    try:
        connection.request(method, path, headers=headers or {})
        response = connection.getresponse()
        return response.status, dict(response.getheaders()), response.read()
    finally:
        connection.close()


@pytest.mark.parametrize("structured", [False, True])
def test_default_server_serves_exact_html_and_no_files(structured):
    if structured:
        source = FIXTURES / "structured.gxw"
        args = ["render-structured", source]
        program = gxwlib.decode_structured(gxwlib.load_project(source), 0)
        expected = gxwlib.render_structured(program).to_html()
    else:
        source = CSV
        args = ["render", source, "--input-format", "csv", "--device-profile", "fx"]
        program = gxwlib.load_csv(source, device_profile="fx")
        expected = gxwlib.render_ladder(program).to_html()
    original = source.read_bytes()
    with running_viewer(*args, "--port", "0") as (process, address):
        status, headers, body = request(address)
        assert status == 200 and body == expected.encode("utf-8")
        assert headers["Content-Type"] == "text/html; charset=utf-8"
        assert int(headers["Content-Length"]) == len(body)
        assert headers["Cache-Control"] == "no-store"
        assert request(address, path="/index.html?view=1")[2] == body
        head_status, head_headers, head_body = request(address, "HEAD")
        assert head_status == 200 and head_body == b""
        assert head_headers["Content-Length"] == headers["Content-Length"]
        for path in ["/LICENSE", "/../LICENSE", "/%2e%2e/LICENSE", "/" + source.name]:
            assert request(address, path=path)[0] == 404
        assert request(address, "POST")[0] == 501
        assert request(address, headers={"Host": "unrelated.example"})[0] == 403
        assert request(address, path="/favicon.ico")[0] == 204
        assert process.poll() is None
    assert source.read_bytes() == original


@pytest.mark.skipif(os.name == "nt", reason="subprocess SIGINT requires a POSIX console")
@pytest.mark.parametrize("partial", [False, True])
def test_interrupt_returns_render_status_and_releases_port(partial):
    source = FIXTURES / "raw.gxw" if partial else CSV
    args = ["render", source, "--device-profile", "fx"]
    if not partial:
        args.extend(["--input-format", "csv"])
    with running_viewer(*args) as (process, address):
        assert request(address)[0] == 200
        process.send_signal(signal.SIGINT)
        stdout, stderr = process.communicate(timeout=5)
        assert process.returncode == (3 if partial else 0)
        assert "Viewer stopped." in stderr and "Traceback" not in stderr
        assert not stdout
        with running_viewer(*args, "--port", address.port) as (_, replacement):
            assert replacement.port == address.port
            assert request(replacement)[0] == 200


def test_occupied_port_is_a_cli_error():
    args = ["render", CSV, "--input-format", "csv", "--device-profile", "fx"]
    with running_viewer(*args) as (_, address):
        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "gxwlib",
                *map(str, args),
                "--port",
                str(address.port),
                "--no-browser",
            ],
            capture_output=True,
            text=True,
            timeout=10,
            check=False,
        )
        assert result.returncode == 1 and "gxw:" in result.stderr
        assert not result.stdout and "Traceback" not in result.stderr
        assert request(address)[0] == 200


@pytest.mark.parametrize(
    "options",
    [
        ["--port", "-1"],
        ["--port", "65536"],
        ["--format", "html", "--no-browser"],
        ["--output", "unused.svg", "--port", "0"],
    ],
)
def test_invalid_viewer_options_fail_before_reading_input(options):
    result = subprocess.run(
        [
            sys.executable,
            "-m",
            "gxwlib",
            "render",
            "missing.gxw",
            "--device-profile",
            "fx",
            *options,
        ],
        capture_output=True,
        text=True,
        timeout=10,
        check=False,
    )
    assert result.returncode == 1 and "gxw:" in result.stderr
    assert "No such file" not in result.stderr and not result.stdout


def test_browser_failure_leaves_manual_url_and_server_running(monkeypatch, capsys):
    attempts = []
    attempted = threading.Event()

    def open_browser(url):
        attempts.append(url)
        return False

    original_open = viewer._open_browser

    def notify_browser_finished(url):
        try:
            original_open(url)
        finally:
            attempted.set()

    def serve_forever(server, **kwargs):
        assert attempted.wait(timeout=5)
        assert attempts == [f"http://127.0.0.1:{server.server_port}/"]
        raise KeyboardInterrupt

    monkeypatch.setattr(viewer.webbrowser, "open", open_browser)
    monkeypatch.setattr(viewer, "_open_browser", notify_browser_finished)
    monkeypatch.setattr(viewer.ThreadingHTTPServer, "serve_forever", serve_forever)
    viewer.serve_html("<!doctype html><p>test</p>")
    captured = capsys.readouterr()
    assert captured.out.strip() == attempts[0]
    assert "Viewer stopped." in captured.err
    assert "open the printed URL manually" in captured.err
