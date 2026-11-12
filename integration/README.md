# Integration tests

End-to-end tests that run a real Braintrust SDK against the **real** extension binary —
no Lambda deployment. Each test proves the happy path: an app logs a trace, `flush()`
hands off to the extension, and the extension delivers the trace upstream using the API
key it captured from the SDK's request.

## Run

```bash
./integration/run.sh              # all supported SDKs
./integration/run.sh python       # one SDK
```

Expected:

```
PASS python      invoke=…ms  sink=…ms  key=… (from SDK Bearer)
PASS typescript  invoke=…ms  sink=…ms  key=…
SKIP ruby   (extension has no /otel/v1/traces route)
SKIP go     (…)
SKIP java   (…)
SKIP dotnet (…)
```

## How it works

Each SDK runs in a **self-contained container** (mirrors a Lambda execution environment:
app + extension + mocks all on `127.0.0.1`). The shared harness (`harness/run.py`) starts,
inside the container, four things:

| Component | What it is |
| --- | --- |
| **sink** (`harness/sink.py`) | Mock Braintrust data plane. Records every `/logs3` the extension forwards (body + bearer) and each `/api/apikey/login`. |
| **lambda-mock** (`harness/lambda_mock.py`) | Mock Lambda Extensions API. Just enough (`register` / `telemetry` / `event/next`) for the real binary to boot. |
| **extension** | The real compiled binary, pointed at the sink upstream and the mock Lambda API. |
| **app** (`apps/<sdk>/`) | A tiny HTTP server built with that SDK, exposing the uniform contract below. |

```
run.py ─POST /invoke─▶ app ─SDK flush ─/logs3─▶ extension ─spool+worker─/logs3─▶ sink
                                                                                  │
run.py polls the sink for the marked trace ◀──────────────────────────────────────┘
```

The harness asserts the sink received the marked trace, delivered with the app's own API
key on both the upstream login and the `/logs3` delivery — i.e. the extension carries no
key of its own and submits on the SDK's behalf, exactly as it does in Lambda. Invoke and
time-to-sink are printed for information only; nothing is gated on latency.

### The uniform app contract

Every `apps/<sdk>` app implements the same interface:

- `GET /health` → `200` when ready.
- `POST /invoke` → log one span named `request` with `input = {"prompt": <BT_TEST_MARKER>}`
  via the SDK, `flush()`, then return `200 {"ok": true}`.

## Design: what's cached vs. what's fresh

The two axes are decoupled so each is rebuilt only when *it* changes:

- **The extension binary** is built once (static musl, from the shipping `extension/Dockerfile`)
  and **mounted into the app containers at runtime** — it is not baked into the images. So
  changing the extension recompiles only the binary; the SDK app images are untouched. This
  keeps the extension dev cycle fast.
- **The SDK app images** each install their own runtime + SDK and are layer-cached, keyed by
  the resolved SDK version. `run.sh` gets that version from the app's own
  `apps/<sdk>/latest-version.sh`, so the "what's latest" logic lives with the app (not in CI),
  and pins the image to it — so an image rebuilds only when a new version is published, while
  every run still tests the latest.

The shared harness is also mounted at runtime, so harness edits don't rebuild images either.

## SDK support

| SDK | Status | Why |
| --- | --- | --- |
| Python (`braintrust`) | ✅ tested | Logs via `/logs3` (`api_version 2`). |
| TypeScript/JS (`braintrust`) | ✅ tested | Logs via `/logs3` (`api_version 2`). |
| Ruby, Go, Java, .NET | ⛔ skipped | OpenTelemetry SDKs — export OTLP to `/otel/v1/traces`, which the extension does not proxy. |

The skipped SDKs are defined (see `apps/ruby`, `apps/go`, `apps/java`, `apps/dotnet`) but
their apps are scaffolds and they're not in `run.sh`'s supported list. When the extension
gains an OTLP passthrough route, fill in each app's SDK tracing and add the SDK to
`SUPPORTED` in `run.sh` and to the CI matrix.

## Add a new SDK

1. Create `apps/<sdk>/` with:
   - an app implementing the contract above,
   - a `Dockerfile` (its own runtime + `python3` for the harness + the SDK, keyed by a
     `SDK_VERSION` build-arg, `ENTRYPOINT ["python3", "/harness/run.py"]`, set `SDK_NAME` and `APP_CMD`),
   - a `latest-version.sh` that prints the latest published version of that SDK.
2. Add `<sdk>` to `SUPPORTED` in `run.sh` and to the `matrix.sdk` list in `.github/workflows/ci.yml`.

The shared harness and the extension binary never change.
