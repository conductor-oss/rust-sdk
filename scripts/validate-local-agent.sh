#!/usr/bin/env bash
# Run the Rust agent quickstart against an already-running local Orkes Conductor.
# The LLM provider key stays in the Conductor server's integration configuration.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/validate-local-agent.sh [provider/model] [--preflight]

Uses http://localhost:8080/api. Pass the provider/model as an argument;
the script does not select a model for you.
The model must already be configured as an integration on the local server.

Examples:
  scripts/validate-local-agent.sh openai/gpt-4o-mini
  scripts/validate-local-agent.sh openai/gpt-4o
  scripts/validate-local-agent.sh openai/gpt-4o-mini --preflight
EOF
}

MODEL=""
PREFLIGHT_ONLY=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage; exit 0 ;;
    --preflight) PREFLIGHT_ONLY=1; shift ;;
    -*) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
    *) MODEL="$1"; shift; if [[ $# -gt 0 && "$1" != --preflight ]]; then
         echo 'Provide only one provider/model.' >&2; exit 2
       fi ;;
  esac
done

if [[ -z "$MODEL" ]]; then
  echo 'Pass a configured provider/model as an argument.' >&2
  usage >&2
  exit 2
fi

if [[ ! "$MODEL" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]]; then
  echo "Invalid model '$MODEL'; expected provider/model (for example openai/gpt-4o-mini)." >&2
  exit 2
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
export CONDUCTOR_SERVER_URL=http://localhost:8080/api
# This local server is reachable without SDK authentication. Ignore any credentials
# left in the shell from a remote Orkes cluster.
unset CONDUCTOR_AUTH_KEY CONDUCTOR_AUTH_SECRET

if ! command -v curl >/dev/null 2>&1; then
  echo 'curl is required.' >&2
  exit 1
fi

echo "Checking ${CONDUCTOR_SERVER_URL%/api}/health ..."
if ! curl --fail --silent --show-error --max-time 10 \
  "${CONDUCTOR_SERVER_URL%/api}/health" >/dev/null; then
  echo 'Local Conductor is not healthy on port 8080.' >&2
  exit 1
fi

PROVIDER="${MODEL%%/*}"
MODEL_NAME="${MODEL#*/}"
# Discard the response body: integration records may include provider secrets.
HTTP_STATUS="$(curl --silent --show-error --max-time 10 -o /dev/null -w '%{http_code}' \
  "${CONDUCTOR_SERVER_URL}/integrations/provider/${PROVIDER}/integration/${MODEL_NAME}")"
if [[ "$HTTP_STATUS" != 200 ]]; then
  echo "Model '$MODEL' is not available from the local integration API (HTTP $HTTP_STATUS)." >&2
  echo 'Configure the provider and model in the Conductor UI, or pass an installed provider/model.' >&2
  exit 1
fi
echo "Server and model $MODEL are ready."

if [[ "$PREFLIGHT_ONLY" == 1 ]]; then
  exit 0
fi

if command -v cargo >/dev/null 2>&1; then
  CARGO_BIN="$(command -v cargo)"
elif [[ -x "${HOME}/.cargo/bin/cargo" ]]; then
  CARGO_BIN="${HOME}/.cargo/bin/cargo"
else
  echo 'Rust/Cargo is required. Install a Rust toolchain of at least 1.85.' >&2
  exit 1
fi

cd "$REPO_ROOT"
LOG_FILE="$(mktemp)"
trap 'rm -f "$LOG_FILE"' EXIT
echo "Running agent_quickstart with $MODEL ..."
set +e
"$CARGO_BIN" run --example agent_quickstart --features agents -- "$MODEL" 2>&1 | tee "$LOG_FILE"
RUN_STATUS="${PIPESTATUS[0]}"
set -e

if [[ "$RUN_STATUS" -ne 0 ]] || ! grep -Fxq 'status: COMPLETED' "$LOG_FILE" || \
  ! grep -Eq '^output: .+' "$LOG_FILE" || grep -Fxq 'output: null' "$LOG_FILE"; then
  echo 'Agent validation failed. Inspect the output above and the execution in Conductor.' >&2
  echo 'Check that the server-side model integration can call its provider.' >&2
  exit 1
fi

echo 'PASS: Rust agent completed and returned output on local Conductor.'
