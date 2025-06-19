#!/bin/bash
set -euo pipefail

echo "Waiting for Anvil to be ready..."
until curl --silent --fail http://anvil:8545 -X POST -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' > /dev/null 2>&1; do
  sleep 1
done
echo "Anvil is ready"

TM_BLOCK_INFO=$(curl -sS "http://celestia-validator:26657/block?height=${TM_BLOCK_HEIGHT}")
TM_BLOCK_HASH=$(echo ${TM_BLOCK_INFO} | awk -F'"hash":"' '{print $2}' | cut -d'"' -f1 | head -n1)
echo "Celestia block hash: ${TM_BLOCK_HASH}"

if [ ! -f .deployed ]; then
  echo "Deploying contracts..."
  blobstream0 deploy --tm-height=${TM_BLOCK_HEIGHT} --tm-block-hash=${TM_BLOCK_HASH} --dev | tee .deployed
fi

BLOBSTREAM_ADDRESS=$(awk -F'address:[[:space:]]*' 'END {print $2}' .deployed)
echo "Blobstream address: ${BLOBSTREAM_ADDRESS}"
exec blobstream0 service --eth-address "${BLOBSTREAM_ADDRESS}"
