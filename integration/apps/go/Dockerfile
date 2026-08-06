# syntax=docker/dockerfile:1
# SCAFFOLD — not built or run in CI (gated behind the "unsupported" compose profile).
# Enable once the extension implements an OTLP /otel/v1/traces passthrough route.
FROM bt-ext-base

RUN apt-get update \
    && apt-get install -y --no-install-recommends golang ca-certificates \
    && rm -rf /var/lib/apt/lists/*
# TODO(otlp): add go.mod requiring github.com/braintrustdata/braintrust-sdk-go and build.
COPY integration/apps/go/app.go /app/app.go

ENV SDK_NAME=go
ENV APP_CMD="go run /app/app.go"
