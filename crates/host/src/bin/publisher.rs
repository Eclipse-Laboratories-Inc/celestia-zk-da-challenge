use alloy_primitives::{Address, B256, U256};
use anyhow::{anyhow, ensure, Context, Result};
use celestia_rpc::blobstream::BlobstreamClient;
use celestia_rpc::{Client as CelestiaClient, HeaderClient, ShareClient};
use celestia_types::hash::Hash;
use celestia_types::{AppVersion, ExtendedHeader};
use clap::Parser;
use da_bridge_methods::{DA_BRIDGE_ELF, DA_BRIDGE_ID};
use dotenv::dotenv;
use hana_blobstream::blobstream::SP1BlobstreamDataCommitmentStored;
use hana_proofs::blobstream_inclusion::find_data_commitment;
use itertools::Itertools;
use rangemap::RangeMap;
use risc0_ethereum_contracts::alloy::network::Ethereum;
use risc0_ethereum_contracts::alloy::providers::{Provider, ProviderBuilder, RootProvider};
use risc0_ethereum_contracts::encode_seal;
use risc0_steel::alloy::{
    network::EthereumWallet,
    signers::local::PrivateKeySigner,
    sol,
    sol_types::SolValue,
};
use risc0_steel::{
    ethereum::{EthBlockHeader, EthEvmEnv, ETH_SEPOLIA_CHAIN_SPEC},
    host::BlockNumberOrTag,
    Contract, EvmInput,
};
use risc0_zkvm::{default_prover, Digest, ExecutorEnv, ProverOpts, VerifierContext};
use std::collections::BTreeMap;
use std::str::FromStr;
use tokio::task;
use toolkit::blobstream::{BinaryMerkleProof, DataRootTuple, IDAOracle};
use toolkit::journal::Journal as ToolkitJournal;
use toolkit::{
    eds_index_to_ods, BlobIndex, BlobProofData, BlobstreamAttestation,
    BlobstreamAttestationAndRowProof, DaFraudGuestData, SpanSequence,
};
use tracing_subscriber::EnvFilter;
use url::Url;
use toolkit::constants::BLOBSTREAM_ADDRESS;

sol!(
    #[sol(rpc, all_derives)]
    "../../contracts/src/Verifier.sol"
);

/// Simple program to create a DA fraud proof for the Verifier contract.
#[derive(Parser)]
struct CliArgs {
    /// Ethereum private key
    #[arg(long, env = "ETH_WALLET_PRIVATE_KEY")]
    eth_wallet_private_key: PrivateKeySigner,

    /// Ethereum RPC endpoint URL
    #[arg(long, env = "ETH_RPC_URL")]
    eth_rpc_url: Url,

    /// Beacon API endpoint URL
    ///
    /// Steel uses a beacon block commitment instead of the execution block.
    /// This allows proofs to be validated using the EIP-4788 beacon roots contract.
    #[cfg(any(feature = "beacon", feature = "history"))]
    #[arg(long, env = "BEACON_API_URL")]
    beacon_api_url: Url,

    /// Ethereum block to use as the state for the contract call
    #[arg(long, env = "EXECUTION_BLOCK", default_value_t = BlockNumberOrTag::Parent)]
    execution_block: BlockNumberOrTag,

    /// Ethereum block to use for the beacon block commitment.
    #[cfg(feature = "history")]
    #[arg(long, env = "COMMITMENT_BLOCK")]
    commitment_block: BlockNumberOrTag,

    /// Celestia RPC endpoint URL
    #[arg(long, env = "CELESTIA_RPC_URL")]
    celestia_rpc_url: Url,

    /// Celestia auth token
    #[arg(long, env = "CELESTIA_AUTH_TOKEN")]
    celestia_auth_token: Option<String>,

    /// Address of the Verifier contract.
    #[arg(long)]
    verifier_address: Address,

    /// Sequence of spans pointing to the index blob.
    #[arg(long)]
    index_blob: SpanSequence,

    /// Sequence of spans pointing to the missing blob. Can be the index blob or any blob
    /// pointed to by the contents of the index blob.
    #[arg(long)]
    challenged_blob: SpanSequence,
}

async fn fetch_blob_proof_data(
    celestia_client: &CelestiaClient,
    blob_commitment: SpanSequence,
    block_header: &ExtendedHeader,
) -> Result<BlobProofData, anyhow::Error> {
    // The start index in SpanSequence is an EDS index.
    let start_share_ods_index = eds_index_to_ods(
        blob_commitment.start,
        block_header.dah.square_width() as u32,
    );

    let share_proof = celestia_client
        .share_get_range(
            block_header,
            start_share_ods_index as u64,
            start_share_ods_index as u64 + blob_commitment.size as u64,
        )
        .await?
        .proof;

    Ok(BlobProofData {
        share_proof,
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

    pub async fn get(
        &mut self,
        block_height: u64, // pass by value; makes life easier
    ) -> Result<&SP1BlobstreamDataCommitmentStored, anyhow::Error> {
        if self.event_cache.get(&block_height).is_none() {
            let event =
                find_data_commitment(block_height, self.blobstream_address, &self.eth_provider)
                    .await
                    .map_err(|e| anyhow::anyhow!("failed to find Blobstream commitment: {e}"))?;

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

async fn fetch_blobstream_attestation(
    celestia_client: &CelestiaClient,
    block_header: &ExtendedHeader,
    blobstream_event_cache: &mut BlobstreamEventCache,
) -> Result<BlobstreamAttestation, anyhow::Error> {
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

async fn fetch_da_bridge_guest_data(
    celestia_client: &CelestiaClient,
    index_blob: SpanSequence,
    challenged_blob: SpanSequence,
    blobstream_event_cache: &mut BlobstreamEventCache,
) -> Result<DaFraudGuestData, anyhow::Error> {
    let index_block_header = celestia_client
        .header_get_by_height(index_blob.height)
        .await?;

    let index_block_proof = fetch_block_proof(
        &celestia_client,
        &index_block_header,
        blobstream_event_cache,
    )
        .await?;

    let mut block_proofs = BTreeMap::new();
    // TODO: find a way to avoid this ugly mutable variable, this works for now
    let mut index_blob_data = None;
    block_proofs.insert(index_blob.height, index_block_proof);

    // Only download the index blob and additional data if the challenge targets a blob inside
    // the index
    if index_blob != challenged_blob {
        let index_blob_proof_data =
            fetch_blob_proof_data(&celestia_client, index_blob, &index_block_header).await?;

        let index = BlobIndex::reconstruct_from_raw(
            index_blob_proof_data.share_proof.shares(),
            AppVersion::V2,
        )?;

        // Assume that the blobs in the index are sorted by block height
        for (block_height, _) in &index.blobs.iter().chunk_by(|blob| blob.height) {
            let block_header = celestia_client.header_get_by_height(block_height).await?;
            let blobstream_attestation =
                fetch_block_proof(&celestia_client, &block_header, blobstream_event_cache).await?;

            block_proofs.insert(block_height, blobstream_attestation);
        }

        index_blob_data = Some(index_blob_proof_data);
    }

    Ok(DaFraudGuestData {
        index_blob,
        challenged_blob,
        index_blob_data,
        block_proofs,
    })
}

/// Performs calls to the Blobstream smart contract to and fetches the data locally.
/// Returns an `EvmInput` struct holding the state required for running Blobstream in ZK.
async fn perform_preflight_calls<P>(
    eth_provider: P,
    blobstream_contract_address: Address,
    blobstream_attestations: &BTreeMap<u64, BlobstreamAttestation>,
    execution_block: BlockNumberOrTag,
    #[cfg(any(feature = "beacon", feature = "history"))] beacon_api_url: Url,
    #[cfg(feature = "history")] commitment_block: BlockNumberOrTag,
) -> Result<EvmInput<EthBlockHeader>>
where
    P: Provider<Ethereum> + 'static,
{
    // ---------------------------------------
    #[cfg(feature = "beacon")]
    log::info!("Beacon commitment to block {}", execution_block);
    #[cfg(feature = "history")]
    log::info!("History commitment to block {}", commitment_block);

    let builder = EthEvmEnv::builder()
        .provider(eth_provider)
        .block_number_or_tag(execution_block);
    #[cfg(any(feature = "beacon", feature = "history"))]
    {
        builder = builder.beacon_api(beacon_api_url.clone());
    }
    #[cfg(feature = "history")]
    {
        builder = builder.commitment_block_number_or_tag(commitment_block);
    }

    let mut env = builder.build().await?;
    //  The `with_chain_spec` method is used to specify the chain configuration.
    env = env.with_chain_spec(&ETH_SEPOLIA_CHAIN_SPEC);

    let mut blobstream_contract = Contract::preflight(blobstream_contract_address, &mut env);

    for (block_height, blobstream_attestation) in blobstream_attestations {
        let data_root_tuple = DataRootTuple {
            height: U256::from(*block_height),
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

    Ok(evm_input)
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let blobstream_contract_address = Address::from_str(BLOBSTREAM_ADDRESS)?;

    // Initialize tracing. In order to view logs, run `RUST_LOG=info cargo run`
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    // Parse the command line arguments.
    let args = CliArgs::try_parse()?;

    // Create an alloy provider for that private key and URL.
    let wallet = EthereumWallet::from(args.eth_wallet_private_key);
    let eth_provider = ProviderBuilder::new()
        .wallet(wallet)
        .on_http(args.eth_rpc_url.clone());

    // Need a different provider for now for Blobstream event filtering
    // TODO: import hana's find_data_commitment() into toolkit
    let ro_provider = RootProvider::connect(args.eth_rpc_url.as_str()).await?;

    let celestia_client = CelestiaClient::new(&args.celestia_rpc_url.to_string(), args.celestia_auth_token.as_deref()).await?;

    let index_blob: SpanSequence = args.index_blob;
    let challenged_blob: SpanSequence = args.challenged_blob;

    let mut blobstream_event_cache =
        BlobstreamEventCache::new(blobstream_contract_address, ro_provider);

    let da_bridge_guest_data = fetch_da_bridge_guest_data(
        &celestia_client,
        index_blob,
        challenged_blob,
        &mut blobstream_event_cache,
    )
    .await?;

    // TODO: this copy is inefficient, adapt the signature of `perform_preflight_calls`
    let blobstream_attestations: BTreeMap<_, _> = da_bridge_guest_data
        .block_proofs
        .iter()
        .map(|(height, block_proof)| (*height, block_proof.blobstream_attestation.clone()))
        .collect();

    // Perform the preflight calls to Blobstream's `verifyAttestation()`
    let evm_input = perform_preflight_calls(
        blobstream_event_cache.eth_provider,
        blobstream_contract_address,
        &blobstream_attestations,
        args.execution_block,
    )
    .await?;

    let serialized_da_guest_data = bincode::serialize(&da_bridge_guest_data)
        .with_context(|| "Failed to serialize DA guest data")?;

    log::info!("Generating proof...");
    let start_time = std::time::Instant::now();

    // Create the steel proof.
    let prove_info = task::spawn_blocking(move || {
        let env = ExecutorEnv::builder()
            .write(&evm_input)?
            .write_frame(&serialized_da_guest_data)
            .build()
            .unwrap();

        default_prover().prove_with_ctx(
            env,
            &VerifierContext::default(),
            DA_BRIDGE_ELF,
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
    let journal = ToolkitJournal::abi_decode(journal, true).context("invalid journal")?;
    log::debug!("Steel commitment: {:?}", journal.commitment);

    // ABI encode the seal.
    let seal = encode_seal(&receipt).context("invalid receipt")?;

    // Create an alloy instance of the Verifier contract.
    let contract = Verifier::new(args.verifier_address, &eth_provider);

    // Call Verifier::getIndexBlobExclusionImageId() to check that the contract has been deployed correctly.
    let contract_image_id = Digest::from(contract.getIndexBlobExclusionImageId().call().await?._0.0);
    ensure!(contract_image_id == DA_BRIDGE_ID.into());

    // For this test, we'll use placeholder values that would need to be computed properly in a real implementation
    
    // Create the challenge proof struct  
    let challenge_proof = ChallengeProof {
        seal: seal.into(),
        imageId: B256::from_slice(Digest::from(DA_BRIDGE_ID).as_bytes()),
        blobstreamProof: IBlobstream::BlobstreamProof {
            height: U256::from(0),
            dataRoot: B256::from([0u8; 32]),
            sideNodes: vec![],
            key: U256::from(0),
            numLeaves: U256::from(0),
        },
        dataRootTupleRoot: B256::from([0u8; 32]),
    };

    // The guest program outputs a Journal with Steel commitment
    // Extract the steel commitment details from the journal
    log::info!(
        "Sending Tx calling challengeIndexBlob Function of {:#}...",
        contract.address()
    );
    let call_builder = contract.challengeIndexBlob(
        journal.indexBlobHash.into(),
        journal.commitment.id,
        journal.commitment.digest,
        journal.commitment.configID,
        challenge_proof
    );
    log::debug!("Send {} {}", contract.address(), call_builder.calldata());
    let pending_tx = call_builder.send().await?;
    let tx_hash = *pending_tx.tx_hash();
    let receipt = pending_tx
        .get_receipt()
        .await
        .with_context(|| format!("transaction did not confirm: {}", tx_hash))?;
    ensure!(receipt.status(), "transaction failed: {}", tx_hash);

    Ok(())
}
