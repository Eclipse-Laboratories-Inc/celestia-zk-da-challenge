//! End-to-end smoke test: prove the contract really lives on chain.

mod fixtures;

use crate::fixtures::{test_env, TestEnv};
use alloy::primitives::U256;
use alloy::providers::Provider;
use rstest::rstest;
use celestia_rpc::{BlobClient, HeaderClient, TxConfig};
use celestia_types::{AppVersion, Blob, nmt::Namespace};

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

#[rstest]
#[tokio::test]
async fn celestia_instance_works(#[future] test_env: TestEnv) {
    let test_env = test_env.await;
    let celestia_client = test_env.celestia_client;

    let chain_head = celestia_client.header_local_head().await.expect("RPC getHead failed");
    println!("chain head: {:?}", chain_head.height());
    println!("chain id: {:?}", chain_head.chain_id());
}

#[rstest]
#[tokio::test]
async fn celestia_submit_blob(#[future] test_env: TestEnv) {
    let test_env = test_env.await;
    let celestia_client = test_env.celestia_client;

    let namespace =
        Namespace::new_v0(&[0xDE, 0xAD, 0xBE, 0xEF]).expect("invalid namespace");
    let blob = Blob::new(namespace, vec![0xCA, 0xFE], AppVersion::V2).expect("invalid blob");

    let height = celestia_client
        .blob_submit(&[blob], TxConfig::default())
        .await
        .expect("failed to submit blob");

    println!("blob submitted at height: {}", height);
}
