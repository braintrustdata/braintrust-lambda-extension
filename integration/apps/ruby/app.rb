# frozen_string_literal: true
#
# SCAFFOLD — not built or run in CI.
#
# The Ruby SDK (`braintrust`) is OpenTelemetry-based and exports spans via OTLP to
# `/otel/v1/traces`, which the extension does not implement. This file exists so that
# enabling Ruby is a drop-in once the extension gains an OTLP passthrough route: fill in
# the SDK tracing below, remove the "unsupported" profile in docker-compose.yml, and
# confirm the toolchain in the Dockerfile.
#
# Uniform contract (same as every SDK app): POST /invoke logs one marked trace + flushes;
# GET /health reports readiness. Marker must land in the span so the sink can find it.

require "webrick"
require "json"

MARKER = ENV.fetch("BT_TEST_MARKER")
PORT = Integer(ENV.fetch("APP_PORT", "8080"))

# TODO(otlp): initialize the Braintrust SDK logger here.

def handle_invoke
  # TODO(otlp): log a span with input: { prompt: MARKER }, then flush synchronously,
  #   e.g. Braintrust::Trace.flush_spans (OTLP → /otel/v1/traces).
  raise NotImplementedError, "Ruby SDK tracing pending extension OTLP support"
end

server = WEBrick::HTTPServer.new(BindAddress: "127.0.0.1", Port: PORT, Logger: WEBrick::Log.new(File::NULL), AccessLog: [])
server.mount_proc("/health") { |_req, res| res.status = 200; res.body = '{"status":"ok"}' }
server.mount_proc("/invoke") do |_req, res|
  handle_invoke
  res.status = 200
  res.body = '{"ok":true}'
end
trap("INT") { server.shutdown }
server.start
