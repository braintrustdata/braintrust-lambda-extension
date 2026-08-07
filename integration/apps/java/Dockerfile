# syntax=docker/dockerfile:1
# SCAFFOLD — not built or run in CI (skipped; the extension has no /otel/v1/traces route).
# Enable once the extension implements an OTLP passthrough. Self-contained like the active
# apps: the extension binary + shared harness are mounted at runtime; python3 runs the harness.
# NOTE: real use needs a Gradle/Maven build pulling dev.braintrust:braintrust-sdk-java;
# this single-file form is a placeholder for the /invoke contract only.
FROM eclipse-temurin:21-jdk
RUN apt-get update && apt-get install -y --no-install-recommends python3 && rm -rf /var/lib/apt/lists/*
COPY integration/apps/java/app.java /app/app.java
ENV SDK_NAME=java
ENV APP_CMD="java /app/app.java"
ENTRYPOINT ["python3", "/harness/run.py"]
