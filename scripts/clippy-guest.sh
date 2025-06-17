#! /usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
ROOT_DIR="${SCRIPT_DIR}/.."

cd "${ROOT_DIR}" && \
  cargo +risc0 clippy \
    --target riscv32im-risc0-zkvm-elf \
    -p da-challenge-guest -- -D warnings --no-deps
