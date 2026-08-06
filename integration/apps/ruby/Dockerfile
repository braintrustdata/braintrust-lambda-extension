# syntax=docker/dockerfile:1
# SCAFFOLD — not built or run in CI (gated behind the "unsupported" compose profile).
# Enable once the extension implements an OTLP /otel/v1/traces passthrough route.
FROM bt-ext-base

RUN apt-get update \
    && apt-get install -y --no-install-recommends ruby ruby-dev build-essential \
    && rm -rf /var/lib/apt/lists/*
# TODO(otlp): install the SDK, e.g.  RUN gem install braintrust
COPY integration/apps/ruby/app.rb /app/app.rb

ENV SDK_NAME=ruby
ENV APP_CMD="ruby /app/app.rb"
