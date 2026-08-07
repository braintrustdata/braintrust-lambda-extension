# syntax=docker/dockerfile:1
# SCAFFOLD — not built or run in CI (skipped; the extension has no /otel/v1/traces route).
# Enable once the extension implements an OTLP passthrough. Self-contained like the active
# apps: the extension binary + shared harness are mounted at runtime; python3 runs the harness.
# NOTE: real use needs a csproj referencing Braintrust.Sdk; this script form is a
# placeholder for the /invoke contract only.
FROM mcr.microsoft.com/dotnet/sdk:8.0
RUN apt-get update && apt-get install -y --no-install-recommends python3 && rm -rf /var/lib/apt/lists/*
COPY integration/apps/dotnet/app.csx /app/app.csx
ENV SDK_NAME=dotnet
ENV APP_CMD="dotnet script /app/app.csx"
ENTRYPOINT ["python3", "/harness/run.py"]
