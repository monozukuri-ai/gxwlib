"""Serve one rendered HTML document on the loopback interface."""

import os
import sys
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from threading import Thread
from urllib.parse import urlsplit


class _ViewerServer(ThreadingHTTPServer):
    allow_reuse_address = os.name != "nt"
    allow_reuse_port = False


def register_options(command):
    command.add_argument(
        "--port", type=int, help="viewer port (default: choose a free port; 0 also chooses one)"
    )
    command.add_argument(
        "--no-browser", action="store_true", help="print the viewer URL without opening a browser"
    )


def wants_server(args):
    serving = args.output is None and args.format is None
    if not serving and (args.port is not None or args.no_browser):
        raise ValueError("--port and --no-browser require viewer mode (no --output or --format)")
    if args.port is not None and not 0 <= args.port <= 65535:
        raise ValueError("port must be between 0 and 65535")
    return serving


def _open_browser(url):
    try:
        if webbrowser.open(url):
            return
    except (webbrowser.Error, OSError):
        pass
    print("gxw: could not open a browser; open the printed URL manually", file=sys.stderr)


def serve_html(html: str, *, port: int = 0, open_browser: bool = True) -> None:
    """Block until Ctrl+C; never expose files from the input or working directory."""
    if not 0 <= port <= 65535:
        raise ValueError("port must be between 0 and 65535")
    payload = html.encode("utf-8")

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            self._respond(body=True)

        def do_HEAD(self):
            self._respond(body=False)

        def _respond(self, *, body):
            # Accept only this local origin, including when a browser sends a
            # different Host to the loopback address (e.g. DNS rebinding).
            actual_port = self.server.server_port
            if self.headers.get("Host") not in {
                f"127.0.0.1:{actual_port}",
                f"localhost:{actual_port}",
            }:
                self.send_error(403)
                return
            path = urlsplit(self.path).path
            if path == "/favicon.ico":
                self.send_response(204)
                self.end_headers()
                return
            if path not in {"/", "/index.html"}:
                self.send_error(404)
                return
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(payload)))
            self.send_header("Cache-Control", "no-store")
            self.send_header("X-Content-Type-Options", "nosniff")
            self.send_header("Referrer-Policy", "no-referrer")
            self.end_headers()
            if body:
                self.wfile.write(payload)

        def log_message(self, format, *args):
            pass

    with _ViewerServer(("127.0.0.1", port), Handler) as server:
        url = f"http://127.0.0.1:{server.server_port}/"
        print(url, flush=True)
        print("Viewer running. Press Ctrl+C to stop.", file=sys.stderr)
        if open_browser:
            Thread(target=_open_browser, args=(url,), daemon=True).start()
        try:
            server.serve_forever(poll_interval=0.1)
        except KeyboardInterrupt:
            print("Viewer stopped.", file=sys.stderr)
