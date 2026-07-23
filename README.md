# braintrust-lambda-layer

An AWS Lambda extension that keeps Braintrust trace delivery off the request path.

The Braintrust SDK flushes traces to this extension over localhost. The extension durably buffers them and delivers them to Braintrust outside the handler's response path, so tracing adds no round-trip latency to the request.

Status: experimental.
