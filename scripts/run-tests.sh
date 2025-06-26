#!/usr/bin/env bash
set -euo pipefail

# Launches the test environment and runs all tests.

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

HELP_MESSAGE="Usage: $(basename "$0") [OPTIONS]

Launches the test environment and runs all tests.

Options:
  -h, --help    Show this help message
  --reset       Reset all containers before running tests"

ROOT_DIR="${SCRIPT_DIR}/.."
DOCKER_COMPOSE_FILE="${ROOT_DIR}/ci/docker-compose.yml"

# Parse command line arguments
RESET=false
while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            echo "$HELP_MESSAGE"
            exit 0
            ;;
        --reset)
            RESET=true
            shift
            ;;
        *)
            shift
            ;;
    esac
done

# Reset containers if flag is set
if [ "$RESET" = true ]; then
    docker compose -f "${DOCKER_COMPOSE_FILE}" down --volumes
fi

# Build the host and guest
cd "${ROOT_DIR}" && cargo build --package cli

cd "${ROOT_DIR}" && forge build
docker compose -f "${DOCKER_COMPOSE_FILE}" up -d

# Run tests sequentially, the current setup is not thread-safe
cd "${ROOT_DIR}" && RUST_LOG=info RISC0_DEV_MODE=1 cargo test -- --test-threads 1
