//! End-to-end smoke test: prove the contract really lives on chain.

mod fixtures;

use rstest::rstest;
use alloy::{
    providers::{ext::AnvilApi, ProviderBuilder},
    primitives::U256,
};
use alloy::providers::DynProvider;
// Bring the fixtures and binding types from the harness crate itself.
// (Hyphens in the package name are turned into underscores by Rust.)
use fixtures::{SimpleStorage};
use crate::fixtures::SimpleStorage::SimpleStorageInstance;

#[rstest]             // rstest injects the fixtures by name
#[tokio::test]        // async test runtime
async fn contract_was_deployed(
    provider: &'static DynProvider,
    deployed_contract: SimpleStorageInstance<DynProvider>,
) {
    // 1) There must be byte-code at the deployed address.
    let code = provider
        .get_code_at(deployed_contract.address(), None)   // latest block
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

    assert_eq!(stored, U256::from(0));
}
