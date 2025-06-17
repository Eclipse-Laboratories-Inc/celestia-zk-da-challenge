//! End-to-end smoke test: prove the contract really lives on chain.

mod fixtures;

use crate::fixtures::Counter::CounterInstance;
use alloy::primitives::U256;
use alloy::providers::{DynProvider, Provider};
use rstest::rstest;
use crate::fixtures::{deployed_contract, provider};

#[rstest]
#[tokio::test]
async fn contract_was_deployed(
    provider: &'static DynProvider,
    #[future] deployed_contract: &'static CounterInstance<(), DynProvider>,
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
        .counter()
        .call()
        .await
        .expect("contract call failed");

    assert_eq!(stored._0, U256::from(0));
}
