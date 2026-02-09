#!/bin/bash
set -e

# Default to localhost if not set
export CONDUCTOR_SERVER_URL=${CONDUCTOR_SERVER_URL:-http://localhost:8080/api}

echo "Starting Kitchensink Workers connecting to $CONDUCTOR_SERVER_URL"
echo "Press Ctrl+C to stop"

cargo run --example kitchensink_workers
