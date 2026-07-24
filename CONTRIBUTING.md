# Contributing

## Design

The layer is a self-contained external Lambda extension that:

- Runs as a native binary, independent of the function's language runtime.
- Serves the Braintrust login, project metadata, `/version`, and `/logs3` endpoints on localhost, so the SDK talks to it instead of the Braintrust data plane.
- Spools accepted `/logs3` payloads to `/tmp`, forwards them to Braintrust, and retries across runtime freezes and warm invocations.

It does not import, patch, or package the Braintrust SDK; it operates at the SDK's HTTP boundary, matching the Braintrust Python SDK 0.27.x.

### Authentication and sessions

The extension has no API key of its own. It uses the `Authorization: Bearer` token from each inbound request, falling back to `BRAINTRUST_API_KEY` if set, so the credential the SDK already holds is the one used upstream. Nothing sensitive needs to be in the function configuration, and the key is kept in memory only: never written to the spool or logged.

The expected setup is a single credential per execution environment but multiple keys work safely. On first use of a key, the extension logs in once and caches the session (the org's login response and its resolved data-plane URL), keyed by that key. Each spooled payload is bound to the key that accepted it and delivered to that key's data plane.

## Prerequisites

- Docker — used to produce the statically linked, Amazon Linux-compatible binary.
- A Rust toolchain — for running tests and lints locally.

## Build the layer

`build-layer.sh` compiles the extension in Docker and packages it as a Lambda layer archive. Build one archive per Lambda architecture:

```bash
BRAINTRUST_LAMBDA_ARCHITECTURE=arm64 \
  ./build-layer.sh dist/braintrust-lambda-layer-arm64.zip

BRAINTRUST_LAMBDA_ARCHITECTURE=x86_64 \
  ./build-layer.sh dist/braintrust-lambda-layer-x86_64.zip
```

With no arguments, the script uses the host architecture and writes `dist/braintrust-lambda-layer.zip`.

Publish an archive with the AWS CLI:

```bash
aws lambda publish-layer-version \
  --layer-name braintrust-tracing-extension \
  --zip-file fileb://dist/braintrust-lambda-layer-arm64.zip \
  --compatible-architectures arm64
```

## Test and lint

The extension is a standard Rust crate under `extension/`:

```bash
cd extension
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

These are the same checks CI runs on every pull request.
