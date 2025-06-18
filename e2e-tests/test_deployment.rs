//! End-to-end smoke test: prove the contract really lives on chain.

mod fixtures;

use crate::fixtures::{test_env, TestEnv};
use alloy::primitives::U256;
use alloy::providers::Provider;
use rstest::rstest;

#[rstest]
#[tokio::test]
async fn contract_was_deployed(#[future] test_env: TestEnv) {
    let test_env = test_env.await;

    let counter_contract = &test_env.counter_contract;
    let provider = &test_env.provider;

    // 1) There must be byte-code at the deployed address.
    let code = provider
        .get_code_at(*counter_contract.address()) // latest block
        .await
        .expect("RPC getCode failed");

    assert!(
        !code.is_empty(),
        "no byte-code found at {:?}",
        counter_contract.address()
    );

    // 2) The public getter should return its default value (0).
    let stored = counter_contract
        .counter()
        .call()
        .await
        .expect("contract call failed");

    assert_eq!(stored._0, U256::from(0));
}
