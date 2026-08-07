#!/usr/bin/env bash
# Run the SDK ↔ extension integration tests. Same command locally and in CI.
#
#   ./integration/run.sh                # all supported SDKs
#   ./integration/run.sh python         # one SDK (CI runs one per matrix job)
#
# The extension binary and the shared harness are mounted into each app container at
# runtime, so they're decoupled from the SDK app images: changing the extension never
# rebuilds an app image, and the app images stay cached until the published SDK changes.
#
# By default each SDK's version is resolved by its own apps/<sdk>/latest-version.sh, and the
# app image is pinned to it so its layer cache busts only when a new version ships.
#
# Env:
#   SDK_VERSION   override the resolved version (pin to this exact version instead).
#   GHA_CACHE=1   use the GitHub Actions layer cache for the app image build (CI only).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

SUPPORTED=(python typescript)
SKIPPED=(ruby go java dotnet)

contains() { local needle="$1"; shift; local e; for e in "$@"; do [ "${e}" = "${needle}" ] && return 0; done; return 1; }

# Default (no args): run the supported SDKs and list the skipped ones for a full picture.
# With explicit args: only those SDKs (a requested skipped SDK still reports SKIP).
if [ "$#" -eq 0 ]; then
  sdks=("${SUPPORTED[@]}" "${SKIPPED[@]}")
else
  sdks=("$@")
fi

BIN_DIR="integration/.bin"
BIN="${BIN_DIR}/braintrust-lambda-extension"

# Build the extension binary from the shipping Dockerfile (static musl, runs in any app
# container). Locally we always build so the current extension source is tested — Docker's
# layer cache makes an unchanged rebuild instant. CI instead provides the binary as an
# artifact and sets EXTENSION_BIN_PREBUILT=1 to use it as-is.
if [ "${EXTENSION_BIN_PREBUILT:-}" = "1" ]; then
  echo "==> Using prebuilt extension binary"
else
  echo "==> Building extension binary"
  docker buildx build --file extension/Dockerfile --output "type=local,dest=${BIN_DIR}" extension
fi
chmod +x "${BIN}"

rc=0
results=()
for sdk in "${sdks[@]}"; do
  if contains "${sdk}" "${SKIPPED[@]}"; then
    results+=("SKIP ${sdk} (extension has no /otel/v1/traces route)")
    continue
  fi
  if ! contains "${sdk}" "${SUPPORTED[@]}"; then
    echo "==> ${sdk}: unknown SDK" >&2
    results+=("FAIL ${sdk} (unknown SDK)")
    rc=1
    continue
  fi
  echo "==> ${sdk}"

  # Each app owns its "latest published version" definition, so this stays generic and new
  # SDKs are drop-in. Pinning to the resolved version makes the image cache bust only when a
  # new version ships. SDK_VERSION overrides; an empty result installs latest unpinned.
  version="${SDK_VERSION:-}"
  resolver="integration/apps/${sdk}/latest-version.sh"
  if [ -z "${version}" ] && [ -x "${resolver}" ]; then
    version="$("${resolver}" 2>/dev/null || true)"
  fi
  [ -n "${version}" ] && echo "    latest ${sdk} braintrust -> ${version}"

  build=(docker buildx build --load -t "bt-app-${sdk}" -f "integration/apps/${sdk}/Dockerfile")
  if [ "${GHA_CACHE:-}" = "1" ]; then
    build+=(--cache-from "type=gha,scope=app-${sdk}" --cache-to "type=gha,mode=max,scope=app-${sdk}")
  fi
  if [ -n "${version}" ]; then
    build+=(--build-arg "SDK_VERSION=${version}")
  fi
  build+=(.)
  "${build[@]}"

  out="$(docker run --rm \
      -v "${REPO_ROOT}/${BIN}:/usr/local/bin/braintrust-lambda-extension:ro" \
      -v "${REPO_ROOT}/integration/harness:/harness:ro" \
      "bt-app-${sdk}" 2>&1)" && status=0 || status=$?
  echo "${out}"
  # Carry the harness's own PASS/FAIL line (with latency) into the summary; fall back to a
  # bare status if it didn't print one.
  line="$(printf '%s\n' "${out}" | grep -E '^(PASS|FAIL) ' | tail -1)"
  if [ "${status}" -eq 0 ]; then
    results+=("${line:-PASS ${sdk}}")
  else
    results+=("${line:-FAIL ${sdk} (exit ${status})}")
    rc=1
  fi
done

echo
echo "==> Summary"
for line in "${results[@]}"; do
  echo "  ${line}"
done

exit "${rc}"
