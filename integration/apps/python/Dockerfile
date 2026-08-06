# syntax=docker/dockerfile:1
# Python SDK test app. FROM the base image (extension binary + shared harness).
FROM bt-ext-base

# Always test the latest published SDK (the point of this check). If a bad release ever
# blocks CI, pin a version here while it's investigated.
RUN pip install --no-cache-dir braintrust
COPY integration/apps/python/app.py /app/app.py

ENV SDK_NAME=python
ENV APP_CMD="python3 /app/app.py"
# ENTRYPOINT (python3 /harness/run.py) is inherited from bt-ext-base.
