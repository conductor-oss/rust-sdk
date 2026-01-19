#!/bin/bash
set -e

echo "🚀 Conductor Rust SDK Test Runner"
echo "=================================="
echo ""

# Check if conductor is running
if ! curl -s http://localhost:8080/health > /dev/null 2>&1; then
    echo "⚠️  Conductor server not detected on localhost:8080"
    echo ""
    echo "Starting Conductor with Docker..."
    docker run -d --name conductor -p 8080:8080 -p 5000:5000 conductoross/conductor-standalone:latest
    
    echo "Waiting for Conductor to be ready..."
    timeout 60 bash -c 'until curl -s http://localhost:8080/health | grep -q healthy; do 
        echo -n "."
        sleep 2
    done' || {
        echo "❌ Conductor failed to start"
        docker logs conductor
        exit 1
    }
    echo ""
    echo "✅ Conductor is ready!"
else
    echo "✅ Conductor server is already running"
fi

echo ""
echo "Setting environment..."
export CONDUCTOR_SERVER_URL=http://localhost:8080/api

echo "Running tests..."
echo ""

# Run tests based on argument
case "${1:-all}" in
    workflow)
        cargo test --test workflow_client_tests -- --nocapture
        ;;
    task)
        cargo test --test task_client_tests -- --nocapture
        ;;
    integration)
        cargo test --test integration_tests -- --nocapture
        ;;
    worker)
        cargo test --test worker_tests -- --nocapture
        ;;
    quick)
        cargo test --test workflow_client_tests test_start_workflow -- --exact --nocapture
        ;;
    all)
        cargo test --tests
        ;;
    *)
        echo "Usage: ./run_tests.sh [workflow|task|integration|worker|quick|all]"
        exit 1
        ;;
esac

echo ""
echo "✅ Tests complete!"
echo ""
echo "💡 View results in Conductor UI: http://localhost:8080"
echo "💡 Stop Conductor: docker stop conductor && docker rm conductor"
