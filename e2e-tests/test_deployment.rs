//! End-to-end smoke test: prove the contract really lives on chain.

mod fixtures;

use alloy::primitives::U256;
use alloy::providers::{DynProvider, Provider};
use rstest::rstest;
// Bring the fixtures and binding types from the harness crate itself.
// (Hyphens in the package name are turned into underscores by Rust.)
use crate::fixtures::SimpleStorage::SimpleStorageInstance;
use crate::fixtures::{deployed_contract, provider};

#[rstest] // rstest injects the fixtures by name
#[tokio::test] // async test runtime
async fn contract_was_deployed(
    provider: &'static DynProvider,
    #[future] deployed_contract: &'static SimpleStorageInstance<(), DynProvider>,
) {
    let deployed_contract = deployed_contract.await;

    // 1) There must be byte-code at the deployed address.
    let code = provider
        .get_code_at(*deployed_contract.address()) // latest block
        .await
        .expect("RPC getCode failed");

    assert!(
        !code.is_empty(),
        "no byte-code found at {:?}",
        deployed_contract.address()
    );

    // 2) The public getter should return its default value (0).
    let stored = deployed_contract
        .value()
        .call()
        .await
        .expect("contract call failed");

    assert_eq!(stored.value, U256::from(0));
}
