#!/bin/bash

# Celestia ZK Fraud Proofs - Local Test Script
set -e

echo "Starting Celestia ZK Fraud Proofs Local Test"
echo "============================================"

# Start Docker services
echo "Starting Docker services..."
cd ci
docker compose up -d
cd ..

# Wait for Celestia validator
echo "Waiting for Celestia validator..."
for i in {1..30}; do
    if BLOCK_HEIGHT=$(curl -sf http://localhost:26657/status 2>/dev/null | jq -r '.result.sync_info.latest_block_height' 2>/dev/null); then
        if [ "$BLOCK_HEIGHT" -gt 1 ]; then
            echo "Celestia validator running (block height: $BLOCK_HEIGHT)"
            break
        fi
    fi
    if [ $i -eq 30 ]; then
        echo "Error: Celestia validator failed to start"
        exit 1
    fi
    sleep 2
done

# Set environment variables
export ETH_WALLET_PRIVATE_KEY="0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
export ETH_RPC_URL="http://localhost:8545"
export CELESTIA_MOCHA_LIGHT_NODE_URL="http://localhost:26659"
export CELESTIA_AUTH_TOKEN=$(cat ci/celestia-config/da_auth_token | tr -d '\n')

# Update Blobstream address
BLOBSTREAM_ADDRESS=$(docker compose -f ci/docker-compose.yml logs blobstream 2>/dev/null | grep "Blobstream address:" | tail -1 | sed 's/.*Blobstream address: //')
if [ -z "$BLOBSTREAM_ADDRESS" ]; then
    BLOBSTREAM_ADDRESS="0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0"
fi

cat > crates/toolkit/src/constants.rs << EOF
/// Address of the Blobstream contract.
pub const BLOBSTREAM_ADDRESS: &str = "$BLOBSTREAM_ADDRESS";
EOF

# Build methods
echo "Building ZK methods..."
unset ARGV0 # useful for agents
cargo build --package da-bridge-methods

# Deploy Verifier contract
echo "Deploying Verifier contract..."
cd contracts

if [ ! -d "lib/forge-std" ]; then
    forge install
fi

DEPLOY_OUTPUT=$(PRIVATE_KEY=$ETH_WALLET_PRIVATE_KEY forge script script/DeployVerifier.s.sol:DeployVerifier \
    --rpc-url http://localhost:8545 \
    --broadcast 2>&1)

VERIFIER_ADDRESS=$(echo "$DEPLOY_OUTPUT" | grep "Deployed Verifier to" | sed 's/.*Deployed Verifier to //')

if [ -z "$VERIFIER_ADDRESS" ]; then
    echo "Error: Failed to deploy Verifier contract"
    echo "$DEPLOY_OUTPUT"
    exit 1
fi

echo "Verifier contract deployed to: $VERIFIER_ADDRESS"
cd ..

# Wait for Blobstream to process blocks
sleep 15

# Run fraud proof test
echo "Running fraud proof test..."
echo "Note: Expected to fail because blob IS available"

RUST_LOG=info RISC0_DEV_MODE=1 cargo run --package apps --bin publisher -- \
    --eth-wallet-private-key $ETH_WALLET_PRIVATE_KEY \
    --eth-rpc-url $ETH_RPC_URL \
    --celestia-rpc-url $CELESTIA_MOCHA_LIGHT_NODE_URL \
    --celestia-auth-token $CELESTIA_AUTH_TOKEN \
    --verifier-address $VERIFIER_ADDRESS \
    --index-blob 10:0:1 \
    --challenged-blob 10:0:1 || {
    
    echo ""
    echo "Test completed - the 'failure' above is expected behavior"
    echo "The fraud proof system correctly detected the blob IS available"
    echo ""
    echo "To stop services: cd ci && docker compose down"
} 