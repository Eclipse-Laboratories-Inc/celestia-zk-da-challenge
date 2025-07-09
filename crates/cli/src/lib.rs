mod blobstream_data_commitment;

use crate::blobstream_data_commitment::get_first_data_commitment_event;
use crate::ICounter::ICounterInstance;
use alloy_primitives::{Address, B256, U256};
use anyhow::{anyhow, ensure, Context, Result};
use celestia_rpc::blobstream::BlobstreamClient;
use celestia_rpc::{Client as CelestiaClient, HeaderClient, ShareClient};
use celestia_types::hash::Hash;
use celestia_types::{AppVersion, ExtendedHeader};
use da_challenge_guest::{DA_CHALLENGE_GUEST_ELF, DA_CHALLENGE_GUEST_ID};
use hana_blobstream::blobstream::SP1BlobstreamDataCommitmentStored;
use hana_proofs::blobstream_inclusion::find_data_commitment;
use rangemap::RangeMap;
use risc0_ethereum_contracts::alloy::network::Ethereum;
use risc0_ethereum_contracts::alloy::providers::{Provider, RootProvider};
use risc0_ethereum_contracts::encode_seal;
use risc0_steel::alloy::contract::private::{
    Provider as PrivateProvider, Transport as PrivateTransport,
};
use risc0_steel::alloy::providers::Network;
use risc0_steel::alloy::{
    sol,
    sol_types::{SolCall, SolValue},
};
use risc0_steel::config::ChainSpec;
use risc0_steel::host::db::{ProofDb, ProviderDb};
use risc0_steel::host::HostCommit;
use risc0_steel::{
    ethereum::{EthBlockHeader, EthEvmEnv},
    host::BlockNumberOrTag,
    Contract, EvmBlockHeader, EvmEnv, EvmInput,
};
use risc0_zkvm::{default_prover, Digest, ExecutorEnv, ProverOpts, Receipt, VerifierContext};
use std::collections::BTreeMap;
use tokio::task;
use toolkit::blobstream::{
    BinaryMerkleProof, Blobstream0, DataRootTuple, IDAOracle, SP1Blobstream,
};
use toolkit::journal::Journal;
use toolkit::{
    BlobIndex, BlobProofData, BlobstreamAttestation, BlobstreamAttestationAndRowProof,
    BlobstreamImpl, BlobstreamInfo, DaChallengeGuestData, SpanSequence,
};
use tracing_subscriber::EnvFilter;

sol!(
    #[sol(rpc, all_derives)]
    "../../contracts/src/ICounter.sol"
);

async fn fetch_blob_proof_data(
    celestia_client: &CelestiaClient,
    span_sequence: SpanSequence,
    block_header: &ExtendedHeader,
) -> Result<BlobProofData, anyhow::Error> {
    let mut share_proofs = BTreeMap::new();

    let span_sequence_end = span_sequence.end_index_ods()?;

    for share_index in span_sequence.start..span_sequence_end {
        let share_proof = celestia_client
            .share_get_range(block_header, share_index as u64, share_index as u64 + 1)
            .await?
            .proof;

        share_proofs.insert(share_index, share_proof);
    }

    Ok(BlobProofData {
        share_proofs,
        app_version: AppVersion::V2.as_u64(),
    })
}

struct BlobstreamEventCache {
    eth_provider: RootProvider,
    blobstream_address: Address,
    event_cache: RangeMap<u64, SP1BlobstreamDataCommitmentStored>,
}

impl BlobstreamEventCache {
    pub fn new(blobstream_address: Address, eth_provider: RootProvider) -> Self {
        Self {
            blobstream_address,
            eth_provider,
            event_cache: RangeMap::new(),
        }
    }

    pub async fn first_data_commitment_stored_event(
        &self,
    ) -> Result<SP1BlobstreamDataCommitmentStored, anyhow::Error> {
        let chain_id = self.eth_provider.get_chain_id().await?;
        get_first_data_commitment_event(chain_id, self.blobstream_address, &self.eth_provider).await
    }

    pub async fn get(
        &mut self,
        block_height: u64,
    ) -> Result<&SP1BlobstreamDataCommitmentStored, anyhow::Error> {
        if self.event_cache.get(&block_height).is_none() {
            let event =
                find_data_commitment(block_height, self.blobstream_address, &self.eth_provider)
                    .await
                    .map_err(|e| anyhow::anyhow!("failed to find Blobstream commitment: {e}"))?;

            log::info!("found DataCommitmentStored event: {event}");

            self.event_cache
                .insert(event.start_block..event.end_block, event);
        }

        // expect is safe here, the entry should always exist by now.
        Ok(self
            .event_cache
            .get(&block_height)
            .expect("the Blobstream event should be in the cache"))
    }
}

/// Extracts the data root field from a Celestia block header and returns i-t
/// as raw bytes for compatibility with later function calls.
fn get_data_root_from_header(block_header: &ExtendedHeader) -> Result<[u8; 32], anyhow::Error> {
    let data_root = match block_header
        .header
        .data_hash
        .expect("data root should always be present")
    {
        Hash::Sha256(hash) => hash,
        Hash::None => {
            return Err(anyhow!(
                "Valid Celestia block should not have None data root"
            ))
        }
    };

    Ok(data_root)
}

async fn get_first_blobstream_attestation(
    celestia_client: &CelestiaClient,
    blobstream_event_cache: &mut BlobstreamEventCache,
) -> Result<BlobstreamAttestation, anyhow::Error> {
    let first_blobstream_event = blobstream_event_cache
        .first_data_commitment_stored_event()
        .await?;

    let block_header = celestia_client
        .header_get_by_height(first_blobstream_event.start_block)
        .await
        .with_context(|| "failed to fetch Celestia block header for first Blobstream event")?;
    let data_root = get_data_root_from_header(&block_header)?;

    let root_inclusion_proof = celestia_client
        .blobstream_get_data_root_tuple_inclusion_proof(
            first_blobstream_event.start_block,
            first_blobstream_event.start_block,
            first_blobstream_event.end_block,
        )
        .await
        .with_context(|| "Failed to fetch Blobstream proof")?;

    Ok(BlobstreamAttestation {
        data_root,
        height: first_blobstream_event.start_block,
        nonce: first_blobstream_event.proof_nonce.try_into()?,
        proof: root_inclusion_proof,
    })
}

async fn fetch_blobstream_attestation(
    celestia_client: &CelestiaClient,
    block_header: &ExtendedHeader,
    blobstream_event_cache: &mut BlobstreamEventCache,
) -> Result<BlobstreamAttestation, anyhow::Error> {
    let data_root = get_data_root_from_header(block_header)?;
    let block_height: u64 = block_header.height().into();

    let blobstream_event = blobstream_event_cache.get(block_height).await?;

    let root_inclusion_proof = celestia_client
        .blobstream_get_data_root_tuple_inclusion_proof(
            block_height,
            blobstream_event.start_block,
            blobstream_event.end_block,
        )
        .await
        .with_context(|| "Failed to fetch Blobstream proof")?;

    Ok(BlobstreamAttestation {
        data_root,
        height: block_height,
        nonce: blobstream_event.proof_nonce.try_into()?,
        proof: root_inclusion_proof,
    })
}

async fn fetch_block_proof(
    celestia_client: &CelestiaClient,
    block_header: &ExtendedHeader,
    blobstream_event_cache: &mut BlobstreamEventCache,
) -> Result<BlobstreamAttestationAndRowProof, anyhow::Error> {
    let blobstream_attestation =
        fetch_blobstream_attestation(celestia_client, block_header, blobstream_event_cache).await?;

    let row_inclusion_proof = block_header
        .dah
        .row_proof(0..=0)
        .with_context(|| "Failed to generate row proof for row 0")?
        .proofs[0]
        .clone();
    let row_root_node = block_header
        .dah
        .row_root(0)
        .expect("row root 0 should always be present");

    Ok(BlobstreamAttestationAndRowProof {
        blobstream_attestation,
        row_proof: row_inclusion_proof,
        row_root_node,
    })
}

async fn fetch_block_proof_for_blob_in_index(
    celestia_client: &CelestiaClient,
    index: &BlobIndex,
    challenged_blob: SpanSequence,
    blobstream_event_cache: &mut BlobstreamEventCache,
) -> Result<Option<BlobstreamAttestationAndRowProof>, anyhow::Error> {
    for span_sequence in &index.blobs {
        if span_sequence == &challenged_blob {
            let block_header = celestia_client
                .header_get_by_height(span_sequence.height)
                .await?;
            let block_proof =
                fetch_block_proof(celestia_client, &block_header, blobstream_event_cache).await?;
            return Ok(Some(block_proof));
        }
    }

    Ok(None)
}

/// Fetches all the data required to execute the DA challenge guest program.
///
/// This function fetches all the data that it can actually fetch, as a valid DA challenge will
/// be unable to download some data by definition.
async fn fetch_da_challenge_guest_data(
    celestia_client: &CelestiaClient,
    index_blob: SpanSequence,
    challenged_blob: SpanSequence,
    blobstream_event_cache: &mut BlobstreamEventCache,
) -> Result<DaChallengeGuestData, anyhow::Error> {
    // First, check the bounds on the index blob height as an invalid block height would prevent
    // us from fetching any data from Celestia.
    let current_celestia_block_height = celestia_client.header_local_head().await?.height().value();
    let first_blobstream_attestation =
        get_first_blobstream_attestation(celestia_client, blobstream_event_cache).await?;

    if index_blob.height < first_blobstream_attestation.height
        || index_blob.height > current_celestia_block_height
    {
        return Ok(DaChallengeGuestData {
            index_blob,
            challenged_blob,
            index_blob_proof_data: None,
            block_proofs: Default::default(),
            first_blobstream_attestation,
        });
    }

    let index_block_header = celestia_client
        .header_get_by_height(index_blob.height)
        .await?;

    let index_block_proof =
        fetch_block_proof(celestia_client, &index_block_header, blobstream_event_cache).await?;

    let mut block_proofs = BTreeMap::from([(index_blob.height, index_block_proof)]);

    if index_blob == challenged_blob {
        return Ok(DaChallengeGuestData {
            index_blob,
            challenged_blob,
            index_blob_proof_data: None,
            block_proofs,
            first_blobstream_attestation,
        });
    }

    // Only download the index blob and additional data if the challenge targets a blob inside
    // the index
    let index_blob_proof_data =
        fetch_blob_proof_data(celestia_client, index_blob, &index_block_header).await?;

    // The index may not be deserializable. We try here to fetch the Blobstream attestation
    // for the challenged blob, but failing here should not prevent the challenge from proceeding.
    if let Ok(index) =
        BlobIndex::reconstruct_from_raw(index_blob_proof_data.shares(), AppVersion::V2)
    {
        if challenged_blob.height < first_blobstream_attestation.height
            || challenged_blob.height > current_celestia_block_height
        {
            return Ok(DaChallengeGuestData {
                index_blob,
                challenged_blob,
                index_blob_proof_data: Some(index_blob_proof_data),
                block_proofs,
                first_blobstream_attestation,
            });
        }

        if let Some(block_proof) = fetch_block_proof_for_blob_in_index(
            celestia_client,
            &index,
            challenged_blob,
            blobstream_event_cache,
        )
        .await?
        {
            block_proofs.insert(challenged_blob.height, block_proof);
        }
    }

    Ok(DaChallengeGuestData {
        index_blob,
        challenged_blob,
        index_blob_proof_data: Some(index_blob_proof_data),
        block_proofs,
        first_blobstream_attestation,
    })
}

async fn perform_preflight_blobstream_height_call<
    C,
    H: EvmBlockHeader + Clone + Send + 'static,
    N: Network,
    P: Provider<N> + 'static,
>(
    #[allow(clippy::type_complexity)]
    blobstream_contract: &mut Contract<&mut EvmEnv<ProofDb<ProviderDb<N, P>>, H, HostCommit<C>>>,
) -> Result<BlobstreamImpl, anyhow::Error> {
    let latest_height_call = Blobstream0::latestHeightCall {};
    let result = blobstream_contract
        .call_builder(&latest_height_call)
        .call()
        .await;

    if let Ok(_height) = result {
        return Ok(BlobstreamImpl::R0);
    }

    let latest_height_call = SP1Blobstream::latestBlockCall {};
    blobstream_contract
        .call_builder(&latest_height_call)
        .call()
        .await?;

    Ok(BlobstreamImpl::Sp1)
}

/// Performs calls to the Blobstream smart contract and fetches the data locally.
/// Returns an `EvmInput` struct holding the state required for running Blobstream in ZK.
async fn perform_preflight_calls<'a, I, P>(
    eth_provider: P,
    chain_spec: &ChainSpec,
    blobstream_contract_address: Address,
    blobstream_attestations: I,
    execution_block: BlockNumberOrTag,
    #[cfg(any(feature = "beacon", feature = "history"))] beacon_api_url: url::Url,
    #[cfg(feature = "history")] commitment_block: BlockNumberOrTag,
) -> Result<(EvmInput<EthBlockHeader>, BlobstreamInfo)>
where
    I: Iterator<Item = &'a BlobstreamAttestation>,
    P: Provider<Ethereum> + 'static,
{
    // ---------------------------------------
    #[cfg(feature = "beacon")]
    log::info!("Beacon commitment to block {execution_block}");
    #[cfg(feature = "history")]
    log::info!("History commitment to block {commitment_block}");

    let builder = EthEvmEnv::builder()
        .provider(eth_provider)
        .block_number_or_tag(execution_block);
    #[cfg(any(feature = "beacon", feature = "history"))]
    let builder = builder.beacon_api(beacon_api_url.clone());
    #[cfg(feature = "history")]
    let builder = builder.commitment_block_number_or_tag(commitment_block);

    let mut env = builder.build().await?;
    //  The `with_chain_spec` method is used to specify the chain configuration.
    env = env.with_chain_spec(chain_spec);

    let mut blobstream_contract = Contract::preflight(blobstream_contract_address, &mut env);

    let blobstream_impl =
        perform_preflight_blobstream_height_call(&mut blobstream_contract).await?;

    for blobstream_attestation in blobstream_attestations {
        let data_root_tuple = DataRootTuple {
            height: U256::from(blobstream_attestation.height),
            dataRoot: B256::from(blobstream_attestation.data_root),
        };
        let formatted_proof = BinaryMerkleProof::from(blobstream_attestation.proof.clone());

        let blobstream_call = IDAOracle::verifyAttestationCall {
            _tupleRootNonce: U256::from(blobstream_attestation.nonce),
            _tuple: data_root_tuple,
            _proof: formatted_proof,
        };

        // Preflight the call to prepare the input that is required to execute the function in
        // the guest without RPC access. It also returns the result of the call.
        blobstream_contract
            .call_builder(&blobstream_call)
            .call()
            .await?;
    }

    // Finally, construct the input from the environment.
    // There are two options: Use EIP-4788 for verification by providing a Beacon API endpoint,
    // or use the regular `blockhash' opcode.
    let evm_input = env.into_input().await?;
    let blobstream_info = BlobstreamInfo {
        address: blobstream_contract_address,
        implementation: blobstream_impl,
    };

    Ok((evm_input, blobstream_info))
}

/// Challenges the availability of a blob in an Eclipse batch / index.
///
/// The caller can challenge at two levels, using the `challenged_blob` parameter:
/// 1. The span sequence pointing to the index
/// 2. Any span sequence in the index.
///
/// This function will fetch all the necessary data to process the DA challenge in ZK and then
/// execute the DA challenge guest program. If the challenge is successful, a ZK proof is generated.
///
/// This function handles 3 possible cases:
/// 1. The index blob is not available (`challenged_blob = index_blob`)
/// 2. A blob inside the index is not available `challenged_blob = blob inside the index`)
/// 3. The index blob is unreadable (`challenged_blob = any span sequence other than the index`).
///
/// # Arguments
///
/// * `celestia_client`: Celestia RPC client.
/// * `root_provider`: Ethereum RPC client.
/// * `chain_spec`: Ethereum chain specification.
/// * `execution_block`: Block number or tag for execution.
/// * `blobstream_address`: Address of the Blobstream contract.
/// * `index_blob`: Span sequence of the index blob.
/// * `challenged_blob`: Span sequence of the blob to challenge.
///
/// # Returns
///
/// A tuple containing:
/// * The ZK proof receipt
/// * The encoded seal.
#[allow(clippy::too_many_arguments)]
pub async fn challenge_da_commitment(
    celestia_client: &CelestiaClient,
    root_provider: RootProvider,
    chain_spec: ChainSpec,
    execution_block: BlockNumberOrTag,
    blobstream_address: Address,
    index_blob: SpanSequence,
    challenged_blob: SpanSequence,
    #[cfg(any(feature = "beacon", feature = "history"))] beacon_api_url: url::Url,
    #[cfg(feature = "history")] commitment_block: BlockNumberOrTag,
) -> Result<(Receipt, Vec<u8>), anyhow::Error> {
    let mut blobstream_event_cache = BlobstreamEventCache::new(blobstream_address, root_provider);

    let da_challenge_guest_data = fetch_da_challenge_guest_data(
        celestia_client,
        index_blob,
        challenged_blob,
        &mut blobstream_event_cache,
    )
    .await?;

    // Perform the preflight calls to Blobstream's `verifyAttestation()`
    let (evm_input, blobstream_info) = perform_preflight_calls(
        blobstream_event_cache.eth_provider,
        &chain_spec,
        blobstream_address,
        da_challenge_guest_data.blobstream_attestations(),
        execution_block,
        #[cfg(any(feature = "beacon", feature = "history"))]
        beacon_api_url,
        #[cfg(feature = "history")]
        commitment_block,
    )
    .await?;

    let serialized_da_guest_data = bincode::serialize(&da_challenge_guest_data)
        .with_context(|| "Failed to serialize DA guest data")?;

    log::info!("Generating proof...");
    let start_time = std::time::Instant::now();

    // Create the steel proof.
    let prove_info = task::spawn_blocking(move || {
        let env = ExecutorEnv::builder()
            .write(&evm_input)?
            .write(&chain_spec)?
            .write(&blobstream_info)?
            .write_frame(&serialized_da_guest_data)
            .build()?;

        default_prover().prove_with_ctx(
            env,
            &VerifierContext::default(),
            DA_CHALLENGE_GUEST_ELF,
            &ProverOpts::groth16(),
        )
    })
    .await?
    .context("failed to create proof")?;

    log::info!(
        "Proof generated in {:.2} s",
        start_time.elapsed().as_secs_f32()
    );
    log::info!("Session stats: {:?}", prove_info.stats);

    let receipt = prove_info.receipt;
    let journal = &receipt.journal.bytes;

    // Decode and log the commitment
    let journal = Journal::abi_decode(journal, true).context("invalid journal")?;
    log::debug!("Steel commitment: {:?}", journal.commitment);

    // ABI encode the seal.
    let seal = encode_seal(&receipt).context("invalid receipt")?;

    Ok((receipt, seal))
}

/// Increments the counter smart contract by providing a valid DA challenge ZK proof.
pub async fn increment_counter<T: Clone + PrivateTransport, P: PrivateProvider<T, Ethereum>>(
    counter_contract: ICounterInstance<T, P>,
    receipt: Receipt,
    seal: Vec<u8>,
) -> Result<(), anyhow::Error> {
    // Call ICounter::imageID() to check that the contract has been deployed correctly.
    let contract_image_id = Digest::from(counter_contract.imageID().call().await?._0.0);
    ensure!(contract_image_id == DA_CHALLENGE_GUEST_ID.into());

    // Call the increment function of the contract and wait for confirmation.
    log::info!(
        "Sending Tx calling {} Function of {:#}...",
        ICounter::incrementCall::SIGNATURE,
        counter_contract.address()
    );
    let call_builder = counter_contract.increment(receipt.journal.bytes.into(), seal.into());
    log::debug!(
        "Send {} {}",
        counter_contract.address(),
        call_builder.calldata()
    );
    let pending_tx = call_builder.send().await?;
    let tx_hash = *pending_tx.tx_hash();
    let receipt = pending_tx
        .get_receipt()
        .await
        .with_context(|| format!("transaction did not confirm: {tx_hash}"))?;
    ensure!(receipt.status(), "transaction failed: {}", tx_hash);

    Ok(())
}

/// Initializes logging.
pub fn logging_init() {
    // Initialize tracing. In order to view logs, run `RUST_LOG=info cargo run`
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init()
        .ok();
}
