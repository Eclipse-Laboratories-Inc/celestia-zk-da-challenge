//! End-to-end smoke test: prove the contract really lives on chain.

mod fixtures;

use crate::fixtures::{test_env, TestEnv};
use alloy::providers::Provider;
use rstest::rstest;
use celestia_rpc::{BlobClient, HeaderClient, TxConfig};
use celestia_types::{AppVersion, Blob, nmt::Namespace};

#[rstest]
#[tokio::test]
async fn blobstream_contract_was_deployed(#[future] test_env: TestEnv) {
    let test_env = test_env.await;
    let provider = &test_env.provider;
    let blobstream_contract = &test_env.blobstream_contract;

    // There must be byte-code at the deployed blobstream address.
    let code = provider
        .get_code_at(*blobstream_contract.address()) // latest block
        .await
        .expect("RPC getCode failed");

    assert!(
        !code.is_empty(),
        "no byte-code found at blobstream address {:?}",
        blobstream_contract.address()
    );
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
