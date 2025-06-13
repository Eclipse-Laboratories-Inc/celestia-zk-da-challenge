use anyhow::Context;
use celestia_rpc::{BlobClient, Client as CelestiaClient, HeaderClient, TxConfig};
use celestia_types::nmt::Namespace;
use celestia_types::{AppVersion, Blob};
use toolkit::{eds_index_to_ods, BlobIndex, SpanSequence};

/// Namespace used for all blobs in this test.
pub const DEFAULT_NAMESPACE: Namespace =
    Namespace::const_v0([0, 0, 0, 0, 0, 0, 0xDE, 0xAD, 0xBE, 0xEF]);

async fn _publish_single_blob(
    celestia_client: &CelestiaClient,
    data: Vec<u8>,
    namespace: Namespace,
) -> Result<SpanSequence, anyhow::Error> {
    let blob =
        Blob::new(namespace, data, AppVersion::V2).with_context(|| "blob creation failed")?;
    let blob_commitment = blob.commitment;
    let height = celestia_client
        .blob_submit(&[blob], TxConfig::default())
        .await
        .with_context(|| "failed to submit blob")?;

    let posted_blob = celestia_client
        .blob_get(height, namespace, blob_commitment)
        .await
        .with_context(|| "failed to fetch blob")?;

    let block_header = celestia_client.header_get_by_height(height).await?;
    let eds_width = block_header.dah.square_width() as u32;

    let start = eds_index_to_ods(posted_blob.index.unwrap() as u32, eds_width);

    Ok(SpanSequence {
        height,
        start,
        size: posted_blob.shares_len() as u32,
    })
}

/// Publishes a single blob and returns the corresponding sequence of spans.
pub async fn publish_single_blob_with_ns(
    celestia_client: &CelestiaClient,
    blob_size: usize,
    namespace: Namespace,
) -> Result<SpanSequence, anyhow::Error> {
    _publish_single_blob(celestia_client, vec![123u8; blob_size], namespace).await
}

pub async fn publish_single_blob(
    celestia_client: &CelestiaClient,
    blob_size: usize,
) -> Result<SpanSequence, anyhow::Error> {
    publish_single_blob_with_ns(celestia_client, blob_size, DEFAULT_NAMESPACE).await
}

pub async fn publish_blobs(
    celestia_client: &CelestiaClient,
    blobs: &[Blob],
    blobs_per_block: usize,
) -> Result<Vec<SpanSequence>, anyhow::Error> {
    let mut blob_spans = vec![];

    for batch in blobs.chunks(blobs_per_block) {
        let height = celestia_client
            .blob_submit(batch, TxConfig::default())
            .await
            .with_context(|| "failed to submit blob")?;

        println!("Blob batch was included at height {height}");

        let block_header = celestia_client.header_get_by_height(height).await?;
        let eds_width = block_header.dah.square_width() as u32;

        for blob in batch {
            let posted_blob = celestia_client
                .blob_get(height, blob.namespace, blob.commitment)
                .await
                .with_context(|| {
                    format!(
                        "failed to retrieve blob {:?} at height {}",
                        blob.commitment, height
                    )
                })?;
            let start = eds_index_to_ods(
                posted_blob.index.expect("posted blob should have an index") as u32,
                eds_width,
            );
            blob_spans.push(SpanSequence {
                height,
                start,
                size: posted_blob.shares_len() as u32,
            });

            println!(
                "Blob {:?} was included at height {} - index {} ({} shares)",
                blob.commitment,
                height,
                start,
                blob.shares_len()
            );
        }
    }

    Ok(blob_spans)
}

pub async fn publish_index(
    celestia_client: &CelestiaClient,
    index: &BlobIndex,
    namespace: Namespace,
) -> Result<SpanSequence, anyhow::Error> {
    let encoded_index =
        bincode::serialize(index).with_context(|| "failed to serialize blob spans")?;
    _publish_single_blob(celestia_client, encoded_index, namespace).await
}

/// Publishes a bunch of blobs and an index blob that points to them.
pub async fn publish_index_blob_with_bad_blob_position(
    celestia_client: &CelestiaClient,
) -> Result<(BlobIndex, SpanSequence), anyhow::Error> {
    // Pick a block height that exists
    let current_celestia_head = celestia_client.header_local_head().await?;
    let ods_width = current_celestia_head.dah.square_width() as u32 / 2;
    let ods_size = ods_width * ods_width;

    let index = BlobIndex::new(vec![SpanSequence {
        height: current_celestia_head.height().value(),
        start: ods_size + 1,
        size: 1,
    }]);

    let index_span_sequence = publish_index(celestia_client, &index, DEFAULT_NAMESPACE).await?;
    Ok((index, index_span_sequence))
}

/// Publishes a bunch of blobs and an index blob that points to them.
pub async fn create_and_publish_index_blob(
    celestia_client: &CelestiaClient,
    n_blobs: usize,
    blob_size: usize,
    blobs_per_block: usize,
) -> Result<(BlobIndex, SpanSequence), anyhow::Error> {
    let blobs = (0..n_blobs)
        .map(|x| {
            Blob::new(DEFAULT_NAMESPACE, vec![x as u8; blob_size], AppVersion::V2)
                .with_context(|| "blob creation failed")
        })
        .collect::<Result<Vec<_>, _>>()?;

    let blob_spans = publish_blobs(celestia_client, &blobs, blobs_per_block).await?;

    let index = BlobIndex::new(blob_spans);
    let index_span_sequence = publish_index(celestia_client, &index, DEFAULT_NAMESPACE).await?;
    Ok((index, index_span_sequence))
}
