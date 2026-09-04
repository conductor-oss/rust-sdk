#!/usr/bin/env bash
#
# Spin up a local Conductor OSS stack and run the SDK integration/worker/
# performance test suites against it, mirroring the `integration-tests-oss` job
# in .github/workflows/ci.yml. Orkes-Enterprise-only tests (orkes_client_tests,
# authorization_client_tests) skip themselves via ApiClient::is_oss().
#
# The stack (Conductor OSS + Postgres) is defined in
# scripts/docker-compose-oss.yaml and is torn down automatically on exit. The
# image is always pulled before starting, since `latest` (the local default) is
# a mutable tag and a cached copy would otherwise go stale silently.
#
# Usage:
#   scripts/run-integration-oss.sh [--keep-up] [--version <tag>] [-- cargo test args]
# Examples:
#   scripts/run-integration-oss.sh                       # run against `latest`
#   scripts/run-integration-oss.sh --version 3.32.0-rc18
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

export OSS_CONDUCTOR_VERSION="${OSS_CONDUCTOR_VERSION:-latest}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
COMPOSE_FILE="${SCRIPT_DIR}/docker-compose-oss.yaml"
cd "${REPO_ROOT}"

compose() { docker compose -f "${COMPOSE_FILE}" "$@"; }

cleanup() {
  if [[ "${KEEP_UP}" == "1" ]]; then
    echo "--keep-up set: leaving the OSS stack running. Tear down with:"
    echo "  docker compose -f ${COMPOSE_FILE} down -v"
    return
  fi
  echo "Tearing down Conductor OSS stack..."
  compose down -v || true
}
trap cleanup EXIT

echo "Using conductoross/conductor:${OSS_CONDUCTOR_VERSION}"

# `docker compose up` only pulls an image when it is missing locally, so a
# previously-cached `latest` (or any other mutable tag) would silently be reused
# instead of getting the current version. Pull unconditionally so the stack
# always reflects the tag we just printed.
echo "Pulling conductoross/conductor:${OSS_CONDUCTOR_VERSION} to ensure it's current..."
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
    compose logs conductor-server || true
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
