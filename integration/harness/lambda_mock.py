"""Mock AWS Lambda Extensions API.

Just enough of the Runtime/Extensions API for the real extension binary to boot:
register, subscribe to telemetry, and long-poll `event/next`. We hold the long-poll
open until teardown, then return a SHUTDOWN event so the extension exits cleanly.
The extension's own delivery worker runs independently of these events, so we do not
need to simulate INVOKE/runtimeDone to observe delivery.
"""

import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def start_lambda_mock(host="127.0.0.1"):
    """Start the mock on an ephemeral port. Returns (httpd, port, shutdown_event)."""
    shutdown = threading.Event()

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def _json(self, code, obj, extra_headers=None):
            payload = json.dumps(obj).encode()
            self.send_response(code)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            for key, value in (extra_headers or {}).items():
                self.send_header(key, value)
            self.end_headers()
            self.wfile.write(payload)

        def _drain(self):
            length = int(self.headers.get("Content-Length", 0))
            if length:
                self.rfile.read(length)

        def do_POST(self):
            self._drain()
            if self.path == "/2020-01-01/extension/register":
                self._json(200, {}, {"Lambda-Extension-Identifier": "test-ext"})
            else:
                self._json(404, {"error": "not found"})

        def do_PUT(self):
            self._drain()
            if self.path == "/2022-07-01/telemetry":
                self._json(200, {})
            else:
                self._json(404, {"error": "not found"})

        def do_GET(self):
            if self.path == "/2020-01-01/extension/event/next":
                # Hold the long-poll until the harness tears down, then shut down.
                shutdown.wait()
                self._json(200, {"eventType": "SHUTDOWN"})
            else:
                self._json(404, {"error": "not found"})

    httpd = ThreadingHTTPServer((host, 0), Handler)
    port = httpd.server_address[1]
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    return httpd, port, shutdown
