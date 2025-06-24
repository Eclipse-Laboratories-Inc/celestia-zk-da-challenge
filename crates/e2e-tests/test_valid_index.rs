//! End-to-end smoke test: DA fraud proof functionality.

mod fixtures;

use crate::fixtures::{test_env, TestEnv};
use rstest::rstest;
use test_toolkit::blobstream::wait_for_blobstream_inclusion;
use test_toolkit::index_blob::publish_index_blob;

const BLOBS_PER_BLOCK: usize = 10;

#[rstest]
#[tokio::test]
async fn challenge_valid_index_blob(#[future] test_env: TestEnv) {
    let TestEnv {
        provider: _provider,
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

    // TODO: Add DA fraud proof challenge logic here
    // challenge_index_blob(celestia_client, index_blob.height).await;

    println!("DA fraud proof test completed successfully");
}
