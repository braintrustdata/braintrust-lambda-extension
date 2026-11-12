# syntax=docker/dockerfile:1
# SCAFFOLD — not built or run in CI (skipped; the extension has no /otel/v1/traces route).
# Enable once the extension implements an OTLP passthrough. Self-contained like the active
# apps: the extension binary + shared harness are mounted at runtime; python3 runs the harness.
FROM ruby:3.3-slim
RUN apt-get update && apt-get install -y --no-install-recommends python3 && rm -rf /var/lib/apt/lists/*
# TODO(otlp): install the SDK, e.g.  RUN gem install braintrust
COPY integration/apps/ruby/app.rb /app/app.rb
ENV SDK_NAME=ruby
ENV APP_CMD="ruby /app/app.rb"
ENTRYPOINT ["python3", "/harness/run.py"]
