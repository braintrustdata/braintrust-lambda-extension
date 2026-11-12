"""Mock Braintrust data plane (the "sink").

Stands in for the real Braintrust backend the extension forwards to. Records every
`/logs3` request (body + bearer) so the harness can assert what actually got delivered,
and answers `/api/apikey/login` so the extension's upstream login succeeds.
"""

import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class SinkState:
    def __init__(self):
        self.lock = threading.Lock()
        self.logs3 = []  # list of {"body": bytes, "authorization": str | None}
        self.login_auths = []  # Authorization header on each /api/apikey/login
        self.base_url = None  # filled in once the server is bound

    def record_login(self, authorization):
        with self.lock:
            self.login_auths.append(authorization)

    def record_logs3(self, body, authorization):
        with self.lock:
            self.logs3.append({"body": body, "authorization": authorization})

    def logs3_containing(self, marker):
        with self.lock:
            return [r for r in self.logs3 if marker in r["body"]]

    def counts(self):
        with self.lock:
            return len(self.logs3), len(self.login_auths)


def start_sink(host="127.0.0.1"):
    """Start the sink on an ephemeral port. Returns (httpd, port, SinkState)."""
    state = SinkState()

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):  # keep the harness output clean
            pass

        def _json(self, code, obj):
            payload = json.dumps(obj).encode()
            self.send_response(code)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def _body(self):
            length = int(self.headers.get("Content-Length", 0))
            return self.rfile.read(length) if length else b""

        def do_POST(self):
            body = self._body()
            if self.path == "/api/apikey/login":
                state.record_login(self.headers.get("Authorization"))
                # Include the org fields the SDKs read from login (api_url gets rewritten
                # to the extension by the extension itself; the rest are passed through).
                self._json(
                    200,
                    {
                        "org_info": [
                            {
                                "id": "o",
                                "name": "test-org",
                                "api_url": state.base_url,
                                "proxy_url": state.base_url,
                                "realtime_url": state.base_url,
                            }
                        ]
                    },
                )
            elif self.path == "/logs3":
                state.record_logs3(body, self.headers.get("Authorization"))
                self._json(200, {})
            else:
                self._json(404, {"error": "not found"})

        def do_GET(self):
            if self.path == "/health":
                self._json(200, {"status": "ok"})
            else:
                self._json(404, {"error": "not found"})

    httpd = ThreadingHTTPServer((host, 0), Handler)
    port = httpd.server_address[1]
    state.base_url = f"http://{host}:{port}"
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    return httpd, port, state
