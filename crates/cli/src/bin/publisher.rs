use alloy_primitives::Address;
use anyhow::{Context, Result};
use celestia_rpc::Client as CelestiaClient;
use clap::Parser;
use cli::{challenge_da_commitment, increment_counter, logging_init, ICounter};
use dotenv::dotenv;
use risc0_ethereum_contracts::alloy::providers::{ProviderBuilder, RootProvider};
use risc0_steel::alloy::{network::EthereumWallet, signers::local::PrivateKeySigner};
use risc0_steel::ethereum::ETH_SEPOLIA_CHAIN_SPEC;
use risc0_steel::host::BlockNumberOrTag;
use std::str::FromStr;
use toolkit::constants::BLOBSTREAM_ADDRESS;
use toolkit::SpanSequence;
use url::Url;

/// Simple program to create a proof to increment the Counter contract.
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

    /// Address of the Blobstream / counter verifier contract.
    #[arg(long)]
    counter_address: Address,

    /// Sequence of spans pointing to the index blob.
    #[arg(long)]
    index_blob: SpanSequence,

    /// Sequence of spans pointing to the missing blob. Can be the index blob or any blob
    /// pointed to by the contents of the index blob.
    #[arg(long)]
    challenged_blob: SpanSequence,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    logging_init();

    let blobstream_address = Address::from_str(BLOBSTREAM_ADDRESS)?;

    // Parse the command line arguments.
    let args = CliArgs::try_parse()?;

    // Create an alloy provider for that private key and URL.
    let wallet = EthereumWallet::from(args.eth_wallet_private_key);
    let eth_provider = ProviderBuilder::new()
        .wallet(wallet)
        .on_http(args.eth_rpc_url.clone());

    // Need a different provider for now for Blobstream event filtering
    // TODO: import hana's find_data_commitment() into toolkit
    let root_provider = RootProvider::connect(args.eth_rpc_url.as_str()).await?;

    let celestia_url = std::env::var("CELESTIA_MOCHA_LIGHT_NODE_URL")
        .with_context(|| "CELESTIA_MOCHA_LIGHT_NODE_URL must be set")?;
    let celestia_client = CelestiaClient::new(&celestia_url, None).await?;

    let index_blob: SpanSequence = args.index_blob;
    let challenged_blob: SpanSequence = args.challenged_blob;

    // Create an alloy instance of the Counter contract.
    let counter_contract = ICounter::new(args.counter_address, &eth_provider);

    let (receipt, seal) = challenge_da_commitment(
        &celestia_client,
        root_provider,
        ETH_SEPOLIA_CHAIN_SPEC.clone(),
        args.execution_block,
        blobstream_address,
        index_blob,
        challenged_blob,
        #[cfg(any(feature = "beacon", feature = "history"))]
        args.beacon_api_url,
        #[cfg(feature = "history")]
        args.commitment_block,
    )
    .await?;
    increment_counter(counter_contract, receipt, seal).await?;

    Ok(())
}
