//! End-to-end smoke test: prove the contract really lives on chain.

mod fixtures;

use crate::fixtures::{test_env, TestEnv};
use alloy::primitives::U256;
use alloy::providers::Provider;
use rstest::rstest;
use test_toolkit::blobstream::wait_for_blobstream_inclusion;
use test_toolkit::index_blob::publish_index_blob;

const BLOBS_PER_BLOCK: usize = 10;

#[rstest]
#[tokio::test]
async fn challenge_valid_index_blob(#[future] test_env: TestEnv) {
    let TestEnv {
        provider,
        counter_contract,
        blobstream_contract,
        celestia_client,
    } = test_env.await;

    let n_blobs = 3;
    let blob_size = 1024;
    println!("Publishing index blob...");
    let index_blob = publish_index_blob(&celestia_client, n_blobs, blob_size, BLOBS_PER_BLOCK)
        .await
        .expect("failed to publish index blob");

    println!("Waiting for blobstream inclusion...");
    wait_for_blobstream_inclusion(&blobstream_contract, index_blob.height)
        .await
        .expect("failed to wait for blobstream inclusion");
    println!("Blobstream inclusion confirmed.");

    //
    // challenge_index_blob(celestia_client, index_blob.height).await;

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
