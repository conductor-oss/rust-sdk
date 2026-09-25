#!/usr/bin/env bash
#
# Spin up a local Conductor OSS stack and run the SDK integration/worker/
# performance test suites against it, mirroring the `integration-tests-oss` job
# in .github/workflows/ci.yml. Orkes-Enterprise-only tests (orkes_client_tests,
# authorization_client_tests) skip themselves via ApiClient::is_oss().
#
# The stack (Conductor OSS + Postgres) is defined in
# scripts/docker-compose-oss.yaml and is torn down automatically on exit. That
# file's `image:` line is also where the default tag lives -- this script
# applies no default of its own, so a plain run and a fork-PR CI run land on the
# identical image. The image is always pulled before starting, since a tag can
# be mutable and a cached copy would otherwise go stale silently.
#
# Usage:
#   scripts/run-integration-oss.sh [--keep-up] [--version <tag>] [-- cargo test args]
# Examples:
#   scripts/run-integration-oss.sh                       # default tag from the compose file
#   scripts/run-integration-oss.sh --version 3.33.0-rc1
#   scripts/run-integration-oss.sh --keep-up             # leave the stack running afterwards
#   scripts/run-integration-oss.sh -- --test orkes_client_tests
set -euo pipefail

KEEP_UP=0
extra=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --keep-up) KEEP_UP=1; shift ;;
    --version) OSS_CONDUCTOR_VERSION="${2:?--version needs a tag}"; shift 2 ;;
    -h|--help)
      echo "Usage: $0 [--keep-up] [--version <tag>] [-- cargo test args]"
      exit 0
      ;;
    --) shift; extra=("$@"); break ;;
    *) echo "Unknown argument: $1" >&2; exit 1 ;;
  esac
done

# No default is applied here on purpose. The default tag is written once, in the
# `image:` line of scripts/docker-compose-oss.yaml, so leaving OSS_CONDUCTOR_VERSION
# unset lets compose supply it -- the same path a fork PR takes in CI. Only export
# it when the caller actually asked for a specific tag, otherwise a value set but
# not exported in the caller's shell would never reach compose anyway.
if [[ -n "${OSS_CONDUCTOR_VERSION:-}" ]]; then
  export OSS_CONDUCTOR_VERSION
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
COMPOSE_FILE="${SCRIPT_DIR}/docker-compose-oss.yaml"
cd "${REPO_ROOT}"

compose() { docker compose -f "${COMPOSE_FILE}" "$@"; }

cleanup() {
  local status=$?
  if [[ "${status}" -ne 0 ]]; then
    echo "Dumping conductor-server logs (exit ${status})..." >&2
    compose logs conductor-server || true
  fi
  if [[ "${KEEP_UP}" == "1" ]]; then
    echo "--keep-up set: leaving the OSS stack running. Tear down with:"
    echo "  docker compose -f ${COMPOSE_FILE} down -v"
    return
  fi
  echo "Tearing down Conductor OSS stack..."
  compose down -v || true
}
trap cleanup EXIT

# Ask compose what it resolved rather than reconstructing the tag here, so this
# stays correct whether the tag came from --version or from the compose default.
# `--images` lists every service's image and does not reliably honour a service
# filter, so select the server's by name rather than by position.
SERVER_IMAGE="$(compose config --images | grep -m1 '^conductoross/conductor:')"
echo "Using ${SERVER_IMAGE}"

# `docker compose up` only pulls an image when it is missing locally, so a
# previously-cached mutable tag (a re-pushed rc, or `latest` if that is what was
# asked for) would silently be reused instead of getting the current version.
# Pull unconditionally so the stack always reflects the tag we just printed.
echo "Pulling ${SERVER_IMAGE} to ensure it's current..."
compose pull conductor-server

echo "Starting Conductor OSS stack..."
compose up -d

echo "Waiting for Conductor to be healthy..."
# Portable wait loop using bash's built-in SECONDS (macOS has no `timeout`).
HEALTH_TIMEOUT="${HEALTH_TIMEOUT:-180}"
deadline=$(( SECONDS + HEALTH_TIMEOUT ))
until curl -sf http://localhost:8080/health >/dev/null 2>&1; do
  if (( SECONDS >= deadline )); then
    echo "Error: Conductor did not become healthy within ${HEALTH_TIMEOUT}s." >&2
    exit 1
  fi
  sleep 5
done
echo "Conductor is up."

export CONDUCTOR_SERVER_URL="http://localhost:8080/api"

# Plain OSS Conductor has no authentication layer and no /token endpoint. A shell
# that still has these exported for the Orkes suite would send the whole run
# through an auth flow the local server cannot serve, and would also make
# ApiClient::is_oss() answer for the wrong server.
unset CONDUCTOR_AUTH_KEY CONDUCTOR_AUTH_SECRET

# --nocapture so the `println!("Skipping: ...")` lines from the is_oss() gates are
# visible; without it a skipped test is indistinguishable from a passing one.
cargo test --tests --all-features -- --test-threads=1 --nocapture ${extra[@]+"${extra[@]}"}
