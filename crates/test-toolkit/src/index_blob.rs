use anyhow::Context;
use celestia_rpc::{BlobClient, Client as CelestiaClient, TxConfig};
use celestia_types::nmt::Namespace;
use celestia_types::{AppVersion, Blob};
use toolkit::{BlobIndex, SpanSequence};

/// Publishes a bunch of blobs and an index blob that points to them.
pub async fn publish_index_blob(
    celestia_client: &CelestiaClient,
    n_blobs: usize,
    blob_size: usize,
    blobs_per_block: usize,
) -> Result<SpanSequence, anyhow::Error> {
    // let's use the DEADBEEF namespace
    let namespace =
        Namespace::new_v0(&[0xDE, 0xAD, 0xBE, 0xEF]).with_context(|| "invalid namespace")?;

    let blobs = (0..n_blobs)
        .map(|x| {
            Blob::new(namespace, vec![x as u8; blob_size], AppVersion::V2)
                .with_context(|| "Blob creation failed")
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut blob_spans = vec![];

    for batch in blobs.chunks(blobs_per_block) {
        let height = celestia_client
            .blob_submit(batch, TxConfig::default())
            .await
            .with_context(|| "failed to submit blob")?;

        println!("Blob batch was included at height {}", height);

        for blob in batch {
            let posted_blob = celestia_client
                .blob_get(height, namespace, blob.commitment)
                .await
                .with_context(|| {
                    format!(
                        "failed to retrieve blob {:?} at height {}",
                        blob.commitment, height
                    )
                })?;
            blob_spans.push(SpanSequence {
                height,
                start: posted_blob.index.expect("posted blob should have an index") as u32,
                size: posted_blob.shares_len() as u32,
            });

            println!(
                "Blob {:?} was included at height {} - index {} ({} shares)",
                blob.commitment,
                height,
                posted_blob.index.unwrap(),
                blob.shares_len()
            );
        }
    }

    let index = BlobIndex { blobs: blob_spans };
    let encoded_index =
        bincode::serialize(&index).with_context(|| "failed to serialize blob spans")?;
    let index_blob = Blob::new(namespace, encoded_index, AppVersion::V2)
        .with_context(|| "failed to create index blob")?;
    let index_blob_commitment = index_blob.commitment;

    let index_blob_height = celestia_client
        .blob_submit(&[index_blob], TxConfig::default())
        .await
        .with_context(|| "failed to submit blob")?;

    let posted_index_blob = celestia_client
        .blob_get(index_blob_height, namespace, index_blob_commitment)
        .await
        .with_context(|| "failed to fetch index blob")?;

    let start = posted_index_blob
        .index
        .expect("index blob should have an index");
    log::info!(
        "Index blob: {:?} at height {index_blob_height} - index {} ({} shares)",
        index_blob_commitment,
        start,
        posted_index_blob.shares_len(),
    );

    Ok(SpanSequence {
        height: index_blob_height,
        start: start as u32,
        size: posted_index_blob.shares_len() as u32,
    })
}
