# syntax=docker/dockerfile:1
# SCAFFOLD — not built or run in CI (gated behind the "unsupported" compose profile).
# Enable once the extension implements an OTLP /otel/v1/traces passthrough route.
# NOTE: real use needs a csproj referencing Braintrust.Sdk; this script form is a
# placeholder for the /invoke contract only.
FROM bt-ext-base

RUN apt-get update \
    && apt-get install -y --no-install-recommends dotnet-sdk-8.0 \
    && rm -rf /var/lib/apt/lists/*
COPY integration/apps/dotnet/app.csx /app/app.csx

ENV SDK_NAME=dotnet
ENV APP_CMD="dotnet script /app/app.csx"
