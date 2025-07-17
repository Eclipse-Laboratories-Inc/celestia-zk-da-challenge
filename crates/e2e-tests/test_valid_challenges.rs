//! End-to-end smoke test: prove the contract really lives on chain.

use alloy::providers::Provider;
use celestia_rpc::{BlobClient, HeaderClient, TxConfig};
use celestia_types::nmt::Namespace;
use celestia_types::{AppVersion, Blob};
use cli::challenge_da_commitment;
use risc0_steel::host::BlockNumberOrTag;
use rstest::rstest;
use test_toolkit::blobstream::wait_for_blobstream_inclusion_with_timeout;
use test_toolkit::index_blob::{
    create_and_publish_index_blob, publish_index, publish_index_blob_with_bad_blob_position,
    publish_single_blob, DEFAULT_NAMESPACE,
};
use test_toolkit::test_env::{test_env, TestEnv};
use toolkit::{eds_index_to_ods, BlobIndex, DaChallenge, SpanSequence};

/// Size of the user payload in single-share blobs.
const BLOB_USER_DATA_SIZE: usize = 478;

/// Challenges the span sequence of an index blob that points to a Celestia block height out of
/// the Blobstream range.
#[rstest]
#[case(SpanSequence{ height: 0, start: 1, size: 1 })]
#[case(SpanSequence{ height: 1_000_000, start: 1, size: 1 })]
#[tokio::test]
async fn invalid_block_height(#[future] test_env: TestEnv, #[case] span_sequence: SpanSequence) {
    let TestEnv {
        provider,
        counter_contract: _counter_contract,
        blobstream_contract,
        celestia_client,
    } = test_env.await;

    let root_provider = provider.root().clone();
    let chain_spec = TestEnv::chain_spec();

    challenge_da_commitment(
        &celestia_client,
        root_provider,
        chain_spec,
        BlockNumberOrTag::Latest,
        *blobstream_contract.address(),
        span_sequence,
        DaChallenge::IndexIsUnavailable,
    )
    .await
    .expect("challenge should succeed");
}

/// Challenges a span sequence inside the index that points to a Celestia block height out of
/// the Blobstream range.
#[rstest]
#[case(SpanSequence{ height: 0, start: 1, size: 1 })]
#[case(SpanSequence{ height: 1_000_000, start: 1, size: 1 })]
#[tokio::test]
async fn invalid_block_height_in_index(
    #[future] test_env: TestEnv,
    #[case] span_sequence: SpanSequence,
) {
    let TestEnv {
        provider,
        counter_contract: _counter_contract,
        blobstream_contract,
        celestia_client,
    } = test_env.await;

    let index = BlobIndex::new(vec![span_sequence]);
    let index_span_sequence = publish_index(&celestia_client, &index, DEFAULT_NAMESPACE)
        .await
        .expect("failed to publish index");

    wait_for_blobstream_inclusion_with_timeout(
        &blobstream_contract,
        index_span_sequence.height,
        std::time::Duration::from_secs(120),
    )
    .await
    .expect("failed or timed out waiting for blobstream inclusion");

    let root_provider = provider.root().clone();
    let chain_spec = TestEnv::chain_spec();

    challenge_da_commitment(
        &celestia_client,
        root_provider,
        chain_spec,
        BlockNumberOrTag::Latest,
        *blobstream_contract.address(),
        index_span_sequence,
        DaChallenge::BlobInIndexIsUnavailable(span_sequence),
    )
    .await
    .expect("challenge should succeed");
}

/// Challenges an index span sequence that starts out of the data square.
#[rstest]
#[tokio::test]
async fn index_start_out_of_square(#[future] test_env: TestEnv) {
    let TestEnv {
        provider,
        counter_contract: _counter_contract,
        blobstream_contract,
        celestia_client,
    } = test_env.await;

    let (_index, index_span_sequence) = create_and_publish_index_blob(&celestia_client, 4, 1024, 4)
        .await
        .expect("failed to publish blobs");

    let block_header = celestia_client
        .header_get_by_height(index_span_sequence.height)
        .await
        .expect("failed to get block header");
    let eds_width = block_header.dah.square_width() as u32;
    let eds_size = eds_width * eds_width;

    let bad_span_sequence = SpanSequence {
        height: index_span_sequence.height,
        start: eds_size + 1,
        size: index_span_sequence.size,
    };

    wait_for_blobstream_inclusion_with_timeout(
        &blobstream_contract,
        index_span_sequence.height,
        std::time::Duration::from_secs(120),
    )
    .await
    .expect("failed or timed out waiting for blobstream inclusion");

    let root_provider = provider.root().clone();
    let chain_spec = TestEnv::chain_spec();

    challenge_da_commitment(
        &celestia_client,
        root_provider,
        chain_spec,
        BlockNumberOrTag::Latest,
        *blobstream_contract.address(),
        bad_span_sequence,
        DaChallenge::IndexIsUnavailable,
    )
    .await
    .expect("challenge should succeed");
}

/// Challenges an index span sequence that starts inside the data square but ends out of it.
#[rstest]
#[tokio::test]
async fn index_end_out_of_square(#[future] test_env: TestEnv) {
    let TestEnv {
        provider,
        counter_contract: _counter_contract,
        blobstream_contract,
        celestia_client,
    } = test_env.await;

    let (_index, index_span_sequence) = create_and_publish_index_blob(&celestia_client, 4, 1024, 4)
        .await
        .expect("failed to publish blobs");

    let block_header = celestia_client
        .header_get_by_height(index_span_sequence.height)
        .await
        .expect("failed to get block header");
    let eds_width = block_header.dah.square_width() as u32;
    let eds_size = eds_width * eds_width;

    let bad_span_sequence = SpanSequence {
        height: index_span_sequence.height,
        start: eds_size - 2,
        size: 4,
    };

    wait_for_blobstream_inclusion_with_timeout(
        &blobstream_contract,
        index_span_sequence.height,
        std::time::Duration::from_secs(120),
    )
    .await
    .expect("failed or timed out waiting for blobstream inclusion");

    let root_provider = provider.root().clone();
    let chain_spec = TestEnv::chain_spec();

    challenge_da_commitment(
        &celestia_client,
        root_provider,
        chain_spec,
        BlockNumberOrTag::Latest,
        *blobstream_contract.address(),
        bad_span_sequence,
        DaChallenge::IndexIsUnavailable,
    )
    .await
    .expect("challenge should succeed");
}

/// Challenges an index with an invalid `SpanSequence.size` value that would cause a `u32` overflow
/// when added to `SpanSequence.index` to determine the position of the last share.
#[rstest]
#[tokio::test]
async fn index_end_u32_overflow(#[future] test_env: TestEnv) {
    let TestEnv {
        provider,
        counter_contract: _counter_contract,
        blobstream_contract,
        celestia_client,
    } = test_env.await;

    let (_index, index_span_sequence) = create_and_publish_index_blob(&celestia_client, 4, 1024, 4)
        .await
        .expect("failed to publish blobs");

    wait_for_blobstream_inclusion_with_timeout(
        &blobstream_contract,
        index_span_sequence.height,
        std::time::Duration::from_secs(120),
    )
    .await
    .expect("failed or timed out waiting for blobstream inclusion");

    let bad_span_sequence = SpanSequence {
        height: index_span_sequence.height,
        start: index_span_sequence.start,
        size: u32::MAX,
    };

    let root_provider = provider.root().clone();
    let chain_spec = TestEnv::chain_spec();

    challenge_da_commitment(
        &celestia_client,
        root_provider,
        chain_spec,
        BlockNumberOrTag::Latest,
        *blobstream_contract.address(),
        bad_span_sequence,
        DaChallenge::IndexIsUnavailable,
    )
    .await
    .expect("challenge should succeed");
}

/// Challenges an index where the index itself is available, but a blob inside it starts out of
/// the data square (`SpanSequence.index > ods_size`).
#[rstest]
#[tokio::test]
async fn blob_in_index_out_of_square(#[future] test_env: TestEnv) {
    let TestEnv {
        provider,
        counter_contract: _counter_contract,
        blobstream_contract,
        celestia_client,
    } = test_env.await;

    let (index, index_span_sequence) = publish_index_blob_with_bad_blob_position(&celestia_client)
        .await
        .expect("failed to publish blobs");

    let challenged_span_sequence = index.blobs[0];

    wait_for_blobstream_inclusion_with_timeout(
        &blobstream_contract,
        index_span_sequence.height,
        std::time::Duration::from_secs(120),
    )
    .await
    .expect("failed or timed out waiting for blobstream inclusion");

    let root_provider = provider.root().clone();
    let chain_spec = TestEnv::chain_spec();

    challenge_da_commitment(
        &celestia_client,
        root_provider,
        chain_spec,
        BlockNumberOrTag::Latest,
        *blobstream_contract.address(),
        index_span_sequence,
        DaChallenge::BlobInIndexIsUnavailable(challenged_span_sequence),
    )
    .await
    .expect("challenge should succeed");
}

/// Challenges an index blob that spans multiple namespaces (the publisher thought it would be
/// fun to split up his index in N blobs, each with a different namespace).
#[rstest]
#[tokio::test]
async fn index_spans_multiple_namespaces(#[future] test_env: TestEnv) {
    let TestEnv {
        provider,
        counter_contract: _counter_contract,
        blobstream_contract,
        celestia_client,
    } = test_env.await;

    // For this test we create enough blobs to guarantee that the index is larger than a single
    // share. This way, we can try to upload it as two contiguous blobs with different mespaces
    // that do deserialize correctly.
    let current_celestia_height = celestia_client
        .header_local_head()
        .await
        .expect("failed to fetch Celestia head")
        .height()
        .value();
    let fake_blobs: Vec<_> = (0..128)
        .map(|x| SpanSequence {
            height: current_celestia_height,
            start: x,
            size: 1,
        })
        .collect();

    let challenged_span_sequence = fake_blobs[3];

    let index = BlobIndex::new(fake_blobs);
    let serialized_index = bincode::serialize(&index).expect("failed to serialize index");

    println!("serialized index length: {} bytes", serialized_index.len());

    let namespaces: Vec<_> = [
        &[0xDE, 0xAD, 0xBE, 0xEF],
        &[0xCA, 0xFE, 0xBA, 0xBE],
        &[0xBE, 0xEF, 0xCA, 0xFE],
    ]
    .into_iter()
    .map(|ns| Namespace::new_v0(ns).expect("failed to create namespace"))
    .collect();

    let blobs = serialized_index
        .chunks(BLOB_USER_DATA_SIZE)
        .zip(namespaces.iter().cycle())
        .map(|(chunk, namespace)| Blob::new(*namespace, chunk.to_vec(), AppVersion::V2))
        .collect::<Result<Vec<_>, _>>()
        .expect("failed to create blobs");

    for blob in &blobs {
        assert_eq!(blob.shares_len(), 1);
    }

    let block_height = celestia_client
        .blob_submit(&blobs, TxConfig::default())
        .await
        .expect("failed to submit blobs");
    let first_blob = celestia_client
        .blob_get(block_height, namespaces[0], blobs[0].commitment)
        .await
        .expect("failed to get blob");
    let block_header = celestia_client
        .header_get_by_height(block_height)
        .await
        .expect("failed to get block header");

    let eds_width = block_header.dah.square_width() as u32;
    let start = eds_index_to_ods(
        first_blob.index.expect("blob should have an index") as u32,
        eds_width,
    );

    let index_span_sequence = SpanSequence {
        height: block_height,
        start,
        size: blobs.len() as u32,
    };

    wait_for_blobstream_inclusion_with_timeout(
        &blobstream_contract,
        index_span_sequence.height,
        std::time::Duration::from_secs(120),
    )
    .await
    .expect("failed or timed out waiting for blobstream inclusion");

    let root_provider = provider.root().clone();
    let chain_spec = TestEnv::chain_spec();

    challenge_da_commitment(
        &celestia_client,
        root_provider,
        chain_spec,
        BlockNumberOrTag::Latest,
        *blobstream_contract.address(),
        index_span_sequence,
        DaChallenge::BlobInIndexIsUnavailable(challenged_span_sequence),
    )
    .await
    .expect("challenge should succeed");
}

/// Challenges an index blob whose sequence of spans points to available data that cannot
/// be deserialized.
#[rstest]
#[tokio::test]
async fn index_blob_not_deserializable(#[future] test_env: TestEnv) {
    let TestEnv {
        provider,
        counter_contract: _counter_contract,
        blobstream_contract,
        celestia_client,
    } = test_env.await;

    let bad_index_span_sequence = publish_single_blob(&celestia_client, 1024)
        .await
        .expect("failed to publish fake index blob");

    wait_for_blobstream_inclusion_with_timeout(
        &blobstream_contract,
        bad_index_span_sequence.height,
        std::time::Duration::from_secs(120),
    )
    .await
    .expect("failed or timed out waiting for blobstream inclusion");

    let root_provider = provider.root().clone();
    let chain_spec = TestEnv::chain_spec();

    challenge_da_commitment(
        &celestia_client,
        root_provider,
        chain_spec,
        BlockNumberOrTag::Latest,
        *blobstream_contract.address(),
        bad_index_span_sequence,
        DaChallenge::IndexIsUnreadable,
    )
    .await
    .expect("challenge should succeed");
}

/// Challenges an index blob that spans zero shares (`SpanSequence.size = 0`).
#[rstest]
#[tokio::test]
async fn index_blob_spans_zero_shares(#[future] test_env: TestEnv) {
    let TestEnv {
        provider,
        counter_contract: _counter_contract,
        blobstream_contract,
        celestia_client,
    } = test_env.await;

    let (_index, index_span_sequence) = create_and_publish_index_blob(&celestia_client, 4, 1024, 4)
        .await
        .expect("failed to publish blobs");

    let bad_span_sequence = SpanSequence {
        height: index_span_sequence.height,
        start: index_span_sequence.start,
        size: 0,
    };

    wait_for_blobstream_inclusion_with_timeout(
        &blobstream_contract,
        index_span_sequence.height,
        std::time::Duration::from_secs(120),
    )
    .await
    .expect("failed or timed out waiting for blobstream inclusion");

    let root_provider = provider.root().clone();
    let chain_spec = TestEnv::chain_spec();

    challenge_da_commitment(
        &celestia_client,
        root_provider,
        chain_spec,
        BlockNumberOrTag::Latest,
        *blobstream_contract.address(),
        bad_span_sequence,
        DaChallenge::IndexIsUnavailable,
    )
    .await
    .expect("challenge should succeed");
}
