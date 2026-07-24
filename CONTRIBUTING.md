# Contributing

## Design

The layer is a self-contained external Lambda extension that:

- Runs as a native binary, independent of the function's language runtime.
- Serves the Braintrust login, project metadata, `/version`, and `/logs3` endpoints on localhost, so the SDK talks to it instead of the Braintrust data plane.
- Spools accepted `/logs3` payloads to `/tmp`, forwards them to Braintrust, and retries across runtime freezes and warm invocations.

It does not import, patch, or package the Braintrust SDK; it operates at the SDK's HTTP boundary, matching the Braintrust Python SDK 0.27.x.

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
