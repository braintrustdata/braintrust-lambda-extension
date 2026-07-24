# Braintrust Lambda Extension

This Lambda extension supports the Braintrust SDK, improving its performance in AWS Lambda handlers. It acts as a proxy that offloads egress from the handler process.

## Setup

Your function must already package the `braintrust` Python SDK and call `braintrust.flush()` at the end of the handler.

### 1. Attach the layer

Add the published layer ARN for your function's region and architecture. For example, for Python 3.12 `x86_64` in `us-west-2`:

```text
arn:aws:lambda:us-west-2:982534393296:layer:braintrust-tracing-extension:5
```

### 2. Point the SDK at the extension

Set these environment variables so the SDK flushes to the extension instead of the Braintrust data plane:

```text
BRAINTRUST_APP_URL=http://127.0.0.1:49891
BRAINTRUST_API_URL=http://127.0.0.1:49891
BRAINTRUST_PROJECT_ID=<project ID>
```

Provide the API key to the SDK the way your application already does. The extension reads it from the SDK's requests, so it does not need to be set in the function configuration. Setting `BRAINTRUST_API_KEY` as a function environment variable also works, but is not recommended.

Pass `project_id` directly to `init_logger()` when you can. If your code passes only a project name, set `BRAINTRUST_PROJECT_ID` so the extension can answer the SDK's project registration locally.

The same configuration through infrastructure-as-code:

```yaml
# CloudFormation
Resources:
  SearchFunction:
    Type: AWS::Lambda::Function
    Properties:
      Architectures:
        - x86_64
      Layers:
        - arn:aws:lambda:us-west-2:982534393296:layer:braintrust-tracing-extension:5
      Environment:
        Variables:
          BRAINTRUST_APP_URL: http://127.0.0.1:49891
          BRAINTRUST_API_URL: http://127.0.0.1:49891
```

```hcl
# Terraform
resource "aws_lambda_function" "search" {
  architectures = ["x86_64"]
  layers = [
    "arn:aws:lambda:us-west-2:982534393296:layer:braintrust-tracing-extension:5",
  ]

  environment {
    variables = {
      BRAINTRUST_APP_URL = "http://127.0.0.1:49891"
      BRAINTRUST_API_URL = "http://127.0.0.1:49891"
    }
  }
}
```

### 3. Keep the handler's flush()

```python
import braintrust

logger = braintrust.init_logger(project_id="<project ID>")


def handler(event, context):
    try:
        with logger.start_span(name="request") as span:
            span.log(input=event)
            return {"statusCode": 200}
    finally:
        braintrust.flush()
```

With the layer attached, `flush()` waits for a local handoff rather than remote delivery.

> **Links:** Python SDK 0.27.x builds `Span.link()` and `Span.permalink()` from `BRAINTRUST_APP_URL`, so they return a localhost URL with this configuration. Generate a shareable link with `braintrust.permalink(span.export(), app_url="https://www.braintrust.dev")`.

## Configuration

| Variable                                         | Default                            | Purpose                                        |
| ------------------------------------------------ | ---------------------------------- | ---------------------------------------------- |
| `BRAINTRUST_LAMBDA_EXTENSION_PORT`               | `49891`                            | Local intake port                              |
| `BRAINTRUST_LAMBDA_SPOOL_DIR`                    | `/tmp/braintrust-lambda-extension` | Retry spool                                    |
| `BRAINTRUST_LAMBDA_UPSTREAM_URL`                 | unset                              | Explicit upstream data-plane URL               |
| `BRAINTRUST_LAMBDA_UPSTREAM_APP_URL`             | `https://www.braintrust.dev`       | Control-plane URL                              |
| `BRAINTRUST_LAMBDA_REQUEST_TIMEOUT_SECONDS`      | `1.5`                              | Per-request upstream timeout                   |
| `BRAINTRUST_LAMBDA_POST_RUNTIME_TIMEOUT_SECONDS` | `2`                                | Maximum delivery wait after runtime completion |
| `BRAINTRUST_LAMBDA_FSYNC`                        | `1`                                | Fsync a payload before acknowledging localhost |
| `BRAINTRUST_LAMBDA_DISABLE_SDK_OVERFLOW`         | `1`                                | Keep SDK payloads on the local `/logs3` path   |

The SDK's asynchronous publisher can remain enabled; the explicit `braintrust.flush()` is the barrier that ensures all rows reach the extension before Lambda freezes the runtime.

For strict tail latency, set `BRAINTRUST_SYNC_FLUSH=1` so the flush performs one consolidated local handoff without the background publisher, and `BRAINTRUST_NUM_RETRIES=0` to skip the SDK's retry backoff — the extension owns retries once it has accepted a payload.

By default the local `/version` response does not advertise `logs3_payload_max_bytes`, which keeps SDK 0.27.x from routing oversized rows through `/logs3/overflow` (not implemented). Set `BRAINTRUST_LAMBDA_DISABLE_SDK_OVERFLOW=0` to restore payload-limit advertisement.

The extension performs the API-key login against `https://www.braintrust.dev`. Set `BRAINTRUST_LAMBDA_UPSTREAM_APP_URL` for a different control plane, and `BRAINTRUST_ORG_NAME` when an API key belongs to more than one organization.

## Delivery semantics

A `/logs3` response means the payload has been written to the execution environment's `/tmp` storage; the extension retries failed upstream requests during the current or a later warm invocation. This protects delivery across normal Lambda freezes and runtime resets. It does not guarantee delivery if AWS permanently destroys the execution environment before the upstream request succeeds, because `/tmp` is scoped to that environment.

The extension sends upstream concurrently with the handler when possible and continues after `platform.runtimeDone` for the configured post-runtime budget. This removes the remote round trip from response latency, though post-runtime work can still appear in Lambda duration and billing metrics.

The extension proxies project-log tracing through `init_logger()`. Prompt loading, datasets, attachments, experiment logging, and other Braintrust API calls are not proxied.
