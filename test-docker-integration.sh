#!/bin/bash

# Test script for Docker container integration test
# This script demonstrates how to run the Docker integration test

echo "🐳 Docker Container Integration Test"
echo "=================================="

# Check if Docker is available
if ! command -v docker &> /dev/null; then
    echo "❌ Docker is not installed or not in PATH"
    echo "Please install Docker to run this integration test"
    exit 1
fi

# Check if Docker daemon is running
if ! docker info &> /dev/null; then
    echo "❌ Docker daemon is not running"
    echo "Please start Docker daemon to run this integration test"
    exit 1
fi

echo "✅ Docker is available and running"
echo ""
echo "Running Docker container integration test..."
echo "This test will:"
echo "  1. Create a temporary test repository with devcontainer setup"
echo "  2. Set up test database entities (project, task, task_attempt)"
echo "  3. Actually create a Docker container using the create_docker_container function"
echo "  4. Verify the container was created and database was updated"
echo "  5. Clean up resources"
echo ""

# Run the integration test
RUN_DOCKER_TESTS=1 cargo test -p local-deployment --lib container::tests::test_create_docker_container_full_integration -- --nocapture --include-ignored

# Check the exit code
if [ $? -eq 0 ]; then
    echo ""
    echo "🎉 Integration test PASSED!"
    echo "   The create_docker_container function successfully:"
    echo "   • Created a Docker container from a devcontainer setup"
    echo "   • Updated the database with the container reference"
    echo "   • Handled container lifecycle properly"
else
    echo ""
    echo "❌ Integration test FAILED"
    echo "   Check the output above for error details"
    exit 1
fi