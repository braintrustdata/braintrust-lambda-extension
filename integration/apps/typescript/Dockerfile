# syntax=docker/dockerfile:1
# TypeScript/JS SDK test app. FROM the base image (extension binary + shared harness).
FROM bt-ext-base

# The base is Debian-based (python:3.12-slim); add a current Node.js via NodeSource.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl gnupg \
    && curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
# Always test the latest published SDK (the point of this check). If a bad release ever
# blocks CI, pin a version here while it's investigated.
RUN npm install --no-save braintrust@latest
COPY integration/apps/typescript/app.mjs /app/app.mjs

ENV SDK_NAME=typescript
ENV APP_CMD="node /app/app.mjs"
# ENTRYPOINT (python3 /harness/run.py) is inherited from bt-ext-base.
