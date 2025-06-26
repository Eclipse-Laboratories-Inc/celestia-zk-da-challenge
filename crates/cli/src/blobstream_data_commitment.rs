use alloy_primitives::{Address, ChainId, B256, U256};
use futures_util::StreamExt;
use hana_blobstream::blobstream::SP1Blobstream::SP1BlobstreamInstance;
use hana_blobstream::blobstream::SP1BlobstreamDataCommitmentStored;
use risc0_ethereum_contracts::alloy::contract::private::Provider;
use risc0_steel::alloy::contract::private::Transport;
use risc0_steel::alloy::network::Ethereum;
use std::str::FromStr;

const MAINNET_CHAIN_ID: ChainId = 1;
const SEPOLIA_CHAIN_ID: ChainId = 11155111;

/// Filters the [current_block - block_window, current_block] Ethereum block range to find
/// the first Blobstream event in the range.
async fn find_first_data_commitment_event<T: Transport + Clone, P: Provider<T, Ethereum>>(
    blobstream_contract: SP1BlobstreamInstance<T, P>,
    block_window: u64,
) -> Result<SP1BlobstreamDataCommitmentStored, anyhow::Error> {
    let current_block = blobstream_contract.provider().get_block_number().await?;
    let start_block = if current_block > block_window {
        current_block - block_window
    } else {
        1
    };

    let mut event_stream = blobstream_contract
        .DataCommitmentStored_filter()
        .from_block(start_block)
        .to_block(current_block)
        .watch()
        .await?
        .into_stream();

    if let Some(evt) = event_stream.next().await {
        let (event, _) = evt?;
        if event.proofNonce != U256::from(1u64) {
            return Err(anyhow::anyhow!(
                "proofNonce != 1, block window is too small"
            ));
        }

        let sp1_event = SP1BlobstreamDataCommitmentStored {
            proof_nonce: event.proofNonce,
            start_block: event.startBlock,
            end_block: event.endBlock,
            data_commitment: event.dataCommitment,
        };

        log::info!("Found first DataCommitmentStored event for Blobstream: {sp1_event:?}",);
        return Ok(sp1_event);
    }

    Err(anyhow::anyhow!("event stream closed before height reached"))
}

/// Finds the first data commitment event for the specified Blobstream instance.
///
/// To make DA commitments challengeable, we need to ensure that the corresponding Celestia
/// blocks are covered by Blobstream. As Blobstream deployments typically start at some point
/// after the deployment of the Celestia chain itself, this block height will differ for every
/// Celestia instance.
///
/// To avoid filtering through years of events, this function uses hardcoded values for public
/// Ethereum chains and defaults to parsing events only if the chain is not supported.
pub async fn get_first_data_commitment_event<T: Clone + Transport, P: Provider<T, Ethereum>>(
    chain_id: ChainId,
    blobstream_address: Address,
    provider: &P,
) -> Result<SP1BlobstreamDataCommitmentStored, anyhow::Error> {
    let data_commitment = match chain_id {
        SEPOLIA_CHAIN_ID => SP1BlobstreamDataCommitmentStored {
            proof_nonce: U256::from(1u64),
            start_block: 1_560_501,
            end_block: 1_560_600,
            data_commitment: B256::from_str(
                "60cd79d32f2fb32ba0086c2d0f8e00d54364fa93715a4f6b28ed4080ef47f0eb",
            )?,
        },
        MAINNET_CHAIN_ID => SP1BlobstreamDataCommitmentStored {
            proof_nonce: U256::from(1u64),
            start_block: 1_605_975,
            end_block: 1_606_500,
            data_commitment: B256::from_str(
                "e0f22e19a558e8da31aa8ee05f737a3ec2a55f92dc6093f34650c69f4cbd53be",
            )?,
        },
        _ => {
            let blobstream_contract = SP1BlobstreamInstance::new(blobstream_address, provider);
            find_first_data_commitment_event(blobstream_contract, 100_000).await?
        }
    };

    Ok(data_commitment)
}
