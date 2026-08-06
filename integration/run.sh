#!/usr/bin/env bash
# Run the SDK ↔ extension integration tests. Same command locally and in CI.
#
#   ./integration/run.sh
#
# Builds the base image (extension binary + harness) once, then runs each supported
# SDK's self-contained container. OTLP SDKs are reported as SKIP (the extension has no
# /otel/v1/traces route yet). Exits non-zero if any supported SDK fails.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

COMPOSE="docker compose -f integration/docker-compose.yml"
ACTIVE_SDKS=(python typescript)
SKIPPED_SDKS=(ruby go java dotnet)

echo "==> Building base image (extension binary + harness)"
docker build -f integration/Dockerfile.base -t bt-ext-base .

rc=0
results=()
for sdk in "${ACTIVE_SDKS[@]}"; do
  echo "==> ${sdk}"
  if ${COMPOSE} run --rm --build "app-${sdk}"; then
    results+=("PASS ${sdk}")
  else
    results+=("FAIL ${sdk}")
    rc=1
  fi
done

for sdk in "${SKIPPED_SDKS[@]}"; do
  results+=("SKIP ${sdk} (extension has no /otel/v1/traces route)")
done

echo
echo "==> Summary"
for line in "${results[@]}"; do
  echo "  ${line}"
done

exit "${rc}"
