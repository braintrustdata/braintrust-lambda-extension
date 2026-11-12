# syntax=docker/dockerfile:1
# TypeScript/JS SDK test app. Self-contained: Node runtime + the SDK, plus python3 to run
# the shared harness. The extension binary and harness are mounted at runtime (not baked
# in), so extension changes never rebuild this image. The SDK layer is keyed by SDK_VERSION,
# so it only rebuilds when a new version is published (CI passes the resolved latest).
FROM node:20-slim

# python3 runs the shared harness (mounted at /harness at runtime).
RUN apt-get update \
    && apt-get install -y --no-install-recommends python3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
ARG SDK_VERSION=latest
RUN npm install --no-save "braintrust@${SDK_VERSION}"

COPY integration/apps/typescript/app.mjs /app/app.mjs

ENV SDK_NAME=typescript
ENV APP_CMD="node /app/app.mjs"
ENTRYPOINT ["python3", "/harness/run.py"]
