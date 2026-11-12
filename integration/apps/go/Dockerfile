# syntax=docker/dockerfile:1
# SCAFFOLD — not built or run in CI (skipped; the extension has no /otel/v1/traces route).
# Enable once the extension implements an OTLP passthrough. Self-contained like the active
# apps: the extension binary + shared harness are mounted at runtime; python3 runs the harness.
FROM golang:1.23-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends python3 && rm -rf /var/lib/apt/lists/*
# TODO(otlp): add go.mod requiring github.com/braintrustdata/braintrust-sdk-go and build.
COPY integration/apps/go/app.go /app/app.go
ENV SDK_NAME=go
ENV APP_CMD="go run /app/app.go"
ENTRYPOINT ["python3", "/harness/run.py"]
