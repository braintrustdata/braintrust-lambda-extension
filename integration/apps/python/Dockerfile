# syntax=docker/dockerfile:1
# Python SDK test app. Self-contained: its own runtime + the SDK. The extension binary and
# the shared harness are mounted at runtime (not baked in), so extension changes never
# rebuild this image. The SDK layer is keyed by SDK_VERSION, so it only rebuilds when a new
# version is published (CI passes the resolved latest; unset installs latest ad hoc).
FROM python:3.12-slim

ARG SDK_VERSION=
RUN pip install --no-cache-dir "braintrust${SDK_VERSION:+==${SDK_VERSION}}"

COPY integration/apps/python/app.py /app/app.py

ENV SDK_NAME=python
ENV APP_CMD="python3 /app/app.py"
# The harness is mounted at /harness at runtime.
ENTRYPOINT ["python3", "/harness/run.py"]
