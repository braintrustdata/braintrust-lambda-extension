# syntax=docker/dockerfile:1
# SCAFFOLD — not built or run in CI (gated behind the "unsupported" compose profile).
# Enable once the extension implements an OTLP /otel/v1/traces passthrough route.
# NOTE: real use needs a Gradle/Maven build pulling dev.braintrust:braintrust-sdk-java;
# this single-file form is a placeholder for the /invoke contract only.
FROM bt-ext-base

RUN apt-get update \
    && apt-get install -y --no-install-recommends default-jdk \
    && rm -rf /var/lib/apt/lists/*
COPY integration/apps/java/app.java /app/app.java

ENV SDK_NAME=java
# Java 11+ single-file source-launch; a real build replaces this once the SDK is wired in.
ENV APP_CMD="java /app/app.java"
