# Braintrust Lambda Extension

The Lambda extension is a native HTTP proxy for the Braintrust SDK running in AWS Lambda handlers. It helps improve latency by offloading blocking logging operations.

## Compatibility

| SDK             | Package             | Versions    |
| --------------- | ------------------- | ----------- |
| Python          | `braintrust` (PyPI) | >= `0.27.0` |
| TypeScript / JS | `braintrust` (npm)  | >= `3.0.0`  |

_NOTE: This extension covers project-log tracing only. Prompt loading, datasets, attachments, experiment logging, and other Braintrust API calls are not proxied._

_NOTE: OTLP-based SDKs (Ruby, Go, Java .NET) are not supported at this time._

## Setup

Your function must already package a supported Braintrust SDK and flush at the end of the handler.

### 1. Attach the layer

Add the published layer ARN for your function's region and architecture. Replace `<version>` with the version from the publish workflow summary.

For `x86_64` in `us-west-2`:

```text
arn:aws:lambda:us-west-2:872608195481:layer:braintrust-tracing-extension:<version>
```

For `arm64` in `us-west-2`:

```text
arn:aws:lambda:us-west-2:872608195481:layer:braintrust-tracing-extension-arm64:<version>
```

### 2. Point the SDK at the extension

Set these environment variables so the SDK flushes to the extension instead of the Braintrust data plane:

```text
BRAINTRUST_APP_URL=http://127.0.0.1:49891
BRAINTRUST_API_URL=http://127.0.0.1:49891
BRAINTRUST_PROJECT_ID=<project ID>
```

Provide the API key to the SDK the way your application already does. The extension reads it from the SDK's requests, so it does not need to be set in the function configuration. Setting `BRAINTRUST_API_KEY` as a function environment variable also works, but is not recommended.

Set `project_id` when configuring the logger (alternatively `BRAINTRUST_PROJECT_ID`) so the extension can answer the SDK's project registration locally.

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
        - arn:aws:lambda:us-west-2:872608195481:layer:braintrust-tracing-extension:<version>
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
    "arn:aws:lambda:us-west-2:872608195481:layer:braintrust-tracing-extension:<version>",
  ]

  environment {
    variables = {
      BRAINTRUST_APP_URL = "http://127.0.0.1:49891"
      BRAINTRUST_API_URL = "http://127.0.0.1:49891"
    }
  }
}
```

### 3. Flush at the end of the handler

With the layer attached, the SDK's flush waits for a local handoff rather than remote delivery. Keep the flush your handler already performs.

```python
# Python
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

```typescript
// TypeScript / JavaScript
import { initLogger, flush } from "braintrust";

const logger = initLogger({ projectId: "<project ID>" });

export async function handler(event: unknown) {
  return await logger
    .traced(
      async (span) => {
        span.log({ input: event });
        return { statusCode: 200 };
      },
      { name: "request" },
    )
    .finally(() => flush());
}
```

_NOTE: When using the extension, permalinks may reflect `localhost` instead of `braintrust.dev`. You can generate a shareable link against the real app URL with the SDK's permalink helper (e.g. in Python: `braintrust.permalink(span.export(), app_url="https://www.braintrust.dev")`.)_

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

- For strict tail latency, set `BRAINTRUST_SYNC_FLUSH=1` so the flush performs one consolidated local handoff without the background publisher, and `BRAINTRUST_NUM_RETRIES=0` to skip the SDK's retry backoff.
- The extension performs the API-key login against `https://www.braintrust.dev`. Set `BRAINTRUST_LAMBDA_UPSTREAM_APP_URL` for a different control plane, and `BRAINTRUST_ORG_NAME` when an API key belongs to more than one organization.
