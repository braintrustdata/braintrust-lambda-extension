#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUTPUT_PATH_INPUT="${1:-${SCRIPT_DIR}/dist/braintrust-lambda-layer.zip}"
ARCHITECTURE="${BRAINTRUST_LAMBDA_ARCHITECTURE:-$(uname -m)}"

case "${ARCHITECTURE}" in
  arm64 | aarch64)
    ARCHITECTURE="arm64"
    DOCKER_PLATFORM="linux/arm64"
    ;;
  x86_64 | amd64)
    ARCHITECTURE="x86_64"
    DOCKER_PLATFORM="linux/amd64"
    ;;
  *)
    echo "Unsupported Lambda architecture: ${ARCHITECTURE}" >&2
    exit 1
    ;;
esac

BUILD_DIR="${SCRIPT_DIR}/dist/build-${ARCHITECTURE}"
STAGING_DIR="${SCRIPT_DIR}/dist/layer-${ARCHITECTURE}"

mkdir -p "$(dirname "${OUTPUT_PATH_INPUT}")"
OUTPUT_DIRECTORY="$(cd "$(dirname "${OUTPUT_PATH_INPUT}")" && pwd)"
OUTPUT_PATH="${OUTPUT_DIRECTORY}/$(basename "${OUTPUT_PATH_INPUT}")"
rm -rf "${BUILD_DIR}" "${STAGING_DIR}"
mkdir -p "${BUILD_DIR}" "${STAGING_DIR}/extensions"

docker build \
  --quiet \
  --platform "${DOCKER_PLATFORM}" \
  --output "type=local,dest=${BUILD_DIR}" \
  "${SCRIPT_DIR}/extension"
install -m 755 \
  "${BUILD_DIR}/braintrust-lambda-extension" \
  "${STAGING_DIR}/extensions/braintrust-lambda-extension"

rm -f "${OUTPUT_PATH}"
cd "${STAGING_DIR}"
zip -q -r "${OUTPUT_PATH}" extensions

echo "Built ${OUTPUT_PATH} for ${ARCHITECTURE}"
