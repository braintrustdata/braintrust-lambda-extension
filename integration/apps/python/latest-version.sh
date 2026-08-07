#!/usr/bin/env bash
# Print the latest published version of the Python `braintrust` package (PyPI).
set -euo pipefail
curl -sf https://pypi.org/pypi/braintrust/json \
  | python3 -c 'import sys, json; print(json.load(sys.stdin)["info"]["version"])'
