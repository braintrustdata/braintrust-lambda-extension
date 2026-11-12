#!/usr/bin/env bash
# Print the latest published version of the npm `braintrust` package.
set -euo pipefail
npm view braintrust version
