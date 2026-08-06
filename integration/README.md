# Integration tests

End-to-end tests that run a real Braintrust SDK against the **real** extension binary —
no Lambda deployment. Each test proves the happy path: an app logs a trace, `flush()`
hands off to the extension, and the extension delivers the trace upstream.

## Run

```bash
./integration/run.sh
```

Builds the base image once, then runs each supported SDK. Expected:

```
PASS python      invoke=…ms  sink=…ms
PASS typescript  invoke=…ms  sink=…ms
SKIP ruby   (extension has no /otel/v1/traces route)
SKIP go     (…)
SKIP java   (…)
SKIP dotnet (…)
```

Run one SDK directly:

```bash
docker build -f integration/Dockerfile.base -t bt-ext-base .
docker compose -f integration/docker-compose.yml run --rm --build app-python
```

## How it works

Each SDK gets a **self-contained container** (one image per SDK, all `FROM bt-ext-base`).
Inside it, the shared harness (`harness/run.py`) starts four things on `127.0.0.1` — the
same way a Lambda execution environment shares localhost:

| Component | What it is |
| --- | --- |
| **sink** (`harness/sink.py`) | Mock Braintrust data plane. Records every `/logs3` the extension forwards (body + bearer); answers `/api/apikey/login`. |
| **lambda-mock** (`harness/lambda_mock.py`) | Mock Lambda Extensions API. Just enough (`register` / `telemetry` / `event/next`) for the real binary to boot. |
| **extension** | The real compiled binary, pointed at the sink upstream and the mock Lambda API. |
| **app** (`apps/<sdk>/`) | A tiny HTTP server built with that SDK, exposing the uniform contract below. |

```
run.py ─POST /invoke─▶ app ─SDK flush ─/logs3─▶ extension ─spool+worker─/logs3─▶ sink
                                                                                  │
run.py polls the sink for the marked trace ◀──────────────────────────────────────┘
```

The harness asserts the sink received the marked trace, delivered with the app's own API
key (proving the extension captured and forwarded the SDK's bearer). Invoke and
time-to-sink are printed for information only — nothing is gated on latency.

### The uniform app contract

Every `apps/<sdk>` app implements the same interface:

- `GET /health` → `200` when ready.
- `POST /invoke` → log one span named `request` with `input = {"prompt": <BT_TEST_MARKER>}`
  via the SDK, `flush()`, then return `200 {"ok": true}`.

Environment the harness provides: `BRAINTRUST_APP_URL` / `BRAINTRUST_API_URL` (the
extension's localhost intake), `BRAINTRUST_API_KEY`, `BRAINTRUST_PROJECT_ID`,
`BT_TEST_MARKER`, `APP_PORT`.

## SDK support

| SDK | Status | Why |
| --- | --- | --- |
| Python (`braintrust`) | ✅ tested | Logs via `/logs3` (`api_version 2`). |
| TypeScript/JS (`braintrust`) | ✅ tested | Logs via `/logs3` (`api_version 2`). |
| Ruby, Go, Java, .NET | ⛔ skipped | OpenTelemetry SDKs — export OTLP to `/otel/v1/traces`, which the extension does not proxy. |

The skipped SDKs are defined (see `apps/ruby`, `apps/go`, `apps/java`, `apps/dotnet`) but
gated behind the docker-compose `unsupported` profile and their apps are scaffolds. When
the extension gains an OTLP passthrough route, fill in each app's SDK tracing, drop the
`profiles: ["unsupported"]` line, and add the SDK to `ACTIVE_SDKS` in `run.sh`.

## Add a new SDK

1. Create `apps/<sdk>/` with an app implementing the contract above and a `Dockerfile`
   (`FROM bt-ext-base`, install the toolchain + SDK, set `SDK_NAME` and `APP_CMD`).
2. Add an `app-<sdk>` service to `docker-compose.yml`.
3. Add `<sdk>` to `ACTIVE_SDKS` in `run.sh`.

The shared harness never changes.
