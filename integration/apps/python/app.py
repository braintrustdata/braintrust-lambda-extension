#!/usr/bin/env python3
"""Python SDK test app: a Lambda-handler stand-in.

`POST /invoke` logs one marked mock-LLM trace via the Braintrust SDK and flushes.
`GET /health` reports readiness. The unique marker lands in the span's input so the
harness can find it in the `/logs3` payload the extension forwards to the sink.
"""

import json
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import braintrust

MARKER = os.environ["BT_TEST_MARKER"]
PROJECT_ID = os.environ.get("BRAINTRUST_PROJECT_ID", "test-project")
PORT = int(os.environ.get("APP_PORT", "8080"))

logger = braintrust.init_logger(project_id=PROJECT_ID)


def handle_invoke():
    with logger.start_span(name="request") as span:
        span.log(
            input={"prompt": MARKER},
            output={"completion": "hello from the mock LLM"},
        )
    braintrust.flush()


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def _json(self, code, obj):
        payload = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def do_GET(self):
        if self.path == "/health":
            self._json(200, {"status": "ok"})
        else:
            self._json(404, {"error": "not found"})

    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        if length:
            self.rfile.read(length)
        if self.path == "/invoke":
            handle_invoke()
            self._json(200, {"ok": True})
        else:
            self._json(404, {"error": "not found"})


if __name__ == "__main__":
    ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
