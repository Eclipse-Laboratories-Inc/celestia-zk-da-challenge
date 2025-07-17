//! End-to-end smoke test: prove the contract really lives on chain.

use alloy::primitives::Address;
use alloy::providers::Provider;
use celestia_rpc::Client as CelestiaClient;
use cli::{challenge_da_commitment, logging_init};
use risc0_steel::config::ChainSpec;
use risc0_steel::host::BlockNumberOrTag;
use rstest::rstest;
use test_toolkit::blobstream::wait_for_blobstream_inclusion_with_timeout;
use test_toolkit::index_blob::{create_and_publish_index_blob, publish_single_blob};
use test_toolkit::test_env::{test_env, TestEnv};
use toolkit::{DaChallenge, SpanSequence};

const BLOBS_PER_BLOCK: usize = 10;

async fn assert_challenge_error<P: Provider>(
    celestia_client: &CelestiaClient,
    provider: &P,
    blobstream_address: Address,
    index_span_sequence: SpanSequence,
    da_challenge: DaChallenge,
    error_message: &str,
) {
    let current_eth_block = provider
        .get_block_number()
        .await
        .expect("failed to get ETH block height");
    println!("Current ETH block: {}", current_eth_block);

    let chain_spec = ChainSpec::new_single(31337, "Cancun".into());
    let root_provider = provider.root().clone();
    let result = challenge_da_commitment(
        &celestia_client,
        root_provider,
        chain_spec,
        BlockNumberOrTag::Number(current_eth_block),
        blobstream_address,
        index_span_sequence,
        da_challenge,
    )
    .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.root_cause().to_string().contains(error_message),
        "unexpected error: {}",
        err.root_cause()
    );
}

async fn assert_blob_is_available<P: Provider>(
    celestia_client: &CelestiaClient,
    provider: &P,
    blobstream_address: Address,
    index_span_sequence: SpanSequence,
    da_challenge: DaChallenge,
) {
    assert_challenge_error(
        celestia_client,
        provider,
        blobstream_address,
        index_span_sequence,
        da_challenge,
        "the specified blob is available, DA challenge failed",
    )
    .await;
}

async fn assert_blob_not_in_index<P: Provider>(
    celestia_client: &CelestiaClient,
    provider: &P,
    blobstream_address: Address,
    index_span_sequence: SpanSequence,
    da_challenge: DaChallenge,
) {
    assert_challenge_error(
        celestia_client,
        provider,
        blobstream_address,
        index_span_sequence,
        da_challenge,
        "the blob under challenge is not part of the specified index",
    )
    .await;
}

/// Challenges a valid index blob. This test expects that the challenge will fail
/// as the index blob is available on Celestia.
#[rstest]
#[tokio::test]
async fn challenge_valid_index_blob(#[future] test_env: TestEnv) {
    logging_init();

    let TestEnv {
        provider,
        counter_contract: _counter_contract,
        blobstream_contract,
        celestia_client,
    } = test_env.await;

    let n_blobs = 3;
    let blob_size = 1024;
    println!("Publishing index blob...");
    let (index, index_span_sequence) =
        create_and_publish_index_blob(&celestia_client, n_blobs, blob_size, BLOBS_PER_BLOCK)
            .await
            .expect("failed to publish index blob");

    println!("Waiting for blobstream inclusion...");
    wait_for_blobstream_inclusion_with_timeout(
        &blobstream_contract,
        index_span_sequence.height,
        std::time::Duration::from_secs(120),
    )
    .await
    .expect("failed or timed out waiting for blobstream inclusion");
    println!("Blobstream inclusion confirmed.");

    assert_blob_is_available(
        &celestia_client,
        &provider,
        *blobstream_contract.address(),
        index_span_sequence,
        DaChallenge::IndexIsUnavailable,
    )
    .await;

    for span_sequence in index.blobs {
        assert_blob_is_available(
            &celestia_client,
            &provider,
            *blobstream_contract.address(),
            index_span_sequence,
            DaChallenge::BlobInIndexIsUnavailable(span_sequence),
        )
        .await;
    }
}

/// Challenges a blob that is not part of the index blob. This test expects that the challenge
/// will fail as the blob is not part of the index blob.
#[rstest]
#[tokio::test]
async fn challenge_blob_not_in_index(#[future] test_env: TestEnv) {
    logging_init();

    let TestEnv {
        provider,
        counter_contract: _counter_contract,
        blobstream_contract,
        celestia_client,
    } = test_env.await;

    let n_blobs = 3;
    let blob_size = 1024;
    println!("Publishing index blob...");
    let (_index, index_span_sequence) =
        create_and_publish_index_blob(&celestia_client, n_blobs, blob_size, BLOBS_PER_BLOCK)
            .await
            .expect("failed to publish index blob");

    // Create a valid blob and try to challenge it. It should fail as the blob is out
    // of the index blob.
    let other_span_sequence = publish_single_blob(&celestia_client, 1024)
        .await
        .expect("failed to publish additional blob");

    println!("Waiting for blobstream inclusion...");
    wait_for_blobstream_inclusion_with_timeout(
        &blobstream_contract,
        other_span_sequence.height,
        std::time::Duration::from_secs(120),
    )
    .await
    .expect("failed or timed out waiting for blobstream inclusion");
    println!("Blobstream inclusion confirmed.");

    assert_blob_not_in_index(
        &celestia_client,
        &provider,
        *blobstream_contract.address(),
        index_span_sequence,
        DaChallenge::BlobInIndexIsUnavailable(other_span_sequence),
    )
    .await;
}

#[rstest]
#[ignore = "not implemented yet"]
#[tokio::test]
async fn challenge_altered_with_incomplete_index_shares(#[future] test_env: TestEnv) {
    let _test_env = test_env.await;
    logging_init();
    todo!()
}

#[rstest]
#[ignore = "not implemented yet"]
#[tokio::test]
async fn challenge_with_index_shares_out_of_order(#[future] test_env: TestEnv) {
    let _test_env = test_env.await;
    logging_init();
    todo!()
}
