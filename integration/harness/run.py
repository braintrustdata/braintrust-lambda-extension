#!/usr/bin/env python3
"""Self-contained integration test for one SDK against the real extension binary.

Runs inside a per-SDK container. Starts, all on 127.0.0.1 (mirroring a Lambda
execution environment): the mock Braintrust sink, a mock Lambda Extensions API,
the real extension binary, and the SDK "app". Then invokes the app and asserts the
marked trace reached the sink, delivered with the app's own API key.

Configured via env: SDK_NAME, APP_CMD, EXTENSION_BIN.
Prints "PASS <sdk> ..." / "FAIL <sdk> ..." and exits 0/1.
"""

import os
import socket
import subprocess
import sys
import time
import urllib.request
import uuid

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from sink import start_sink  # noqa: E402
from lambda_mock import start_lambda_mock  # noqa: E402

EXTENSION_BIN = os.environ.get("EXTENSION_BIN", "/usr/local/bin/braintrust-lambda-extension")
PROJECT_ID = "test-project"


def free_port():
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


def wait_http_ok(url, timeout=20.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(url, timeout=1) as resp:
                if resp.status == 200:
                    return True
        except Exception:
            time.sleep(0.1)
    return False


def main():
    sdk = os.environ.get("SDK_NAME", "unknown")
    app_cmd = os.environ.get("APP_CMD")
    if not app_cmd:
        print(f"FAIL {sdk}: APP_CMD not set", flush=True)
        return 1

    marker = f"bt-marker-{uuid.uuid4().hex}"
    api_key = f"bt-test-key-{uuid.uuid4().hex}"

    sink_httpd, sink_port, sink = start_sink()
    lambda_httpd, lambda_port, lambda_shutdown = start_lambda_mock()
    intake_port = free_port()
    app_port = free_port()

    ext_env = dict(os.environ)
    ext_env.update(
        {
            "AWS_LAMBDA_RUNTIME_API": f"127.0.0.1:{lambda_port}",
            "BRAINTRUST_LAMBDA_EXTENSION_PORT": str(intake_port),
            "BRAINTRUST_LAMBDA_UPSTREAM_URL": f"http://127.0.0.1:{sink_port}",
            "BRAINTRUST_LAMBDA_UPSTREAM_APP_URL": f"http://127.0.0.1:{sink_port}",
            "BRAINTRUST_PROJECT_ID": PROJECT_ID,
            "BRAINTRUST_LAMBDA_FSYNC": "0",
        }
    )
    # The extension must capture the SDK's bearer, not carry a key of its own.
    ext_env.pop("BRAINTRUST_API_KEY", None)

    extension = subprocess.Popen([EXTENSION_BIN], env=ext_env)
    app = None
    try:
        if not wait_http_ok(f"http://127.0.0.1:{intake_port}/health"):
            print(f"FAIL {sdk}: extension did not become healthy", flush=True)
            return 1

        app_env = dict(os.environ)
        app_env.update(
            {
                "BRAINTRUST_APP_URL": f"http://127.0.0.1:{intake_port}",
                "BRAINTRUST_API_URL": f"http://127.0.0.1:{intake_port}",
                "BRAINTRUST_API_KEY": api_key,
                "BRAINTRUST_PROJECT_ID": PROJECT_ID,
                "BT_TEST_MARKER": marker,
                "APP_PORT": str(app_port),
            }
        )
        app = subprocess.Popen(app_cmd, env=app_env, shell=True)
        if not wait_http_ok(f"http://127.0.0.1:{app_port}/health"):
            print(f"FAIL {sdk}: app did not become healthy", flush=True)
            return 1

        # Invoke the "handler". flush() should hand off locally and return quickly.
        t0 = time.time()
        req = urllib.request.Request(
            f"http://127.0.0.1:{app_port}/invoke", method="POST", data=b"{}"
        )
        with urllib.request.urlopen(req, timeout=30) as resp:
            invoke_ms = (time.time() - t0) * 1000
            if resp.status != 200:
                print(f"FAIL {sdk}: /invoke returned {resp.status}", flush=True)
                return 1

        # The extension should forward the marked trace to the sink, off the app's path.
        deadline = time.time() + 15
        hits = []
        while time.time() < deadline:
            hits = sink.logs3_containing(marker.encode())
            if hits:
                break
            time.sleep(0.05)
        sink_ms = (time.time() - t0) * 1000

        if not hits:
            n_logs3, n_logins = sink.counts()
            print(
                f"FAIL {sdk}: marker never reached the sink "
                f"(logs3={n_logs3}, logins={n_logins})",
                flush=True,
            )
            return 1

        # The whole point of the extension: it holds no key of its own (we started it
        # without BRAINTRUST_API_KEY) and submits on the SDK's behalf using the bearer it
        # captured from the SDK's requests. Assert the SDK's key reached the sink on both
        # the upstream login and the trace delivery.
        expected = f"Bearer {api_key}"
        logs3_auth = hits[0]["authorization"]
        if logs3_auth != expected:
            print(f"FAIL {sdk}: /logs3 delivered with wrong credential: {logs3_auth!r}", flush=True)
            return 1
        if expected not in sink.login_auths:
            print(
                f"FAIL {sdk}: extension did not log in upstream with the SDK's key "
                f"(saw {sink.login_auths!r})",
                flush=True,
            )
            return 1

        masked = f"{api_key[:20]}…"  # constant prefix + part of the per-run unique suffix
        print(
            f"PASS {sdk}  invoke={invoke_ms:.0f}ms  sink={sink_ms:.0f}ms  "
            f"key={masked} (from SDK Bearer)",
            flush=True,
        )
        return 0
    finally:
        lambda_shutdown.set()  # release the held event/next so the extension can exit
        if app is not None:
            app.terminate()
        try:
            extension.wait(timeout=3)
        except subprocess.TimeoutExpired:
            extension.terminate()
        sink_httpd.shutdown()
        lambda_httpd.shutdown()


if __name__ == "__main__":
    sys.exit(main())
