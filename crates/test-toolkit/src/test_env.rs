//! Shared fixtures and contract binding for the end-to-end test crate.
//!
//! * Requires the `anvil` binary somewhere in `$PATH` (or `foundryup --bin anvil`).
//! * Uses rstest’s `#[once]` so Anvil and the deployment happen **exactly one time**
//!   per test binary run.

use crate::blobstream::get_blobstream_address;
use crate::contracts::Blobstream0;
use crate::contracts::Blobstream0::Blobstream0Instance;
use crate::contracts::Counter;
use crate::contracts::Counter::CounterInstance;
use alloy::network::EthereumWallet;
use alloy::providers::{DynProvider, Provider, ProviderBuilder};
use alloy::signers::local::PrivateKeySigner;
use celestia_rpc::Client as CelestiaClient;
use risc0_steel::config::ChainSpec;
use rstest::*;
use std::str::FromStr;

pub struct TestEnv {
    pub provider: DynProvider,
    pub counter_contract: CounterInstance<(), DynProvider>,
    pub blobstream_contract: Blobstream0Instance<(), DynProvider>,
    pub celestia_client: CelestiaClient,
}

impl TestEnv {
    pub fn chain_spec() -> ChainSpec {
        ChainSpec::new_single(31337, "Cancun".into())
    }
}

async fn deploy_counter(provider: DynProvider) -> CounterInstance<(), DynProvider> {
    let deployer_address = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
        .parse()
        .expect("Failed to parse deployer address");

    // no async #[once] fixture: create a throw-away Tokio runtime inside the call
    Counter::deploy(provider, deployer_address)
        .await
        .expect("Failed to deploy Counter")
}

#[fixture]
pub async fn test_env() -> TestEnv {
    // Use Anvil's first default account
    let private_key = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    let signer = PrivateKeySigner::from_str(private_key).unwrap();
    let wallet = EthereumWallet::from(signer);

    let provider = ProviderBuilder::new()
        .wallet(wallet)
        .connect("http://localhost:8545")
        .await
        .expect("Failed to connect to Anvil")
        .erased();

    let blobstream_address = get_blobstream_address();
    let blobstream_contract = Blobstream0::new(blobstream_address, provider.clone());
    let counter_contract = deploy_counter(provider.clone()).await;

    let celestia_url = "http://localhost:26659";
    let celestia_client = CelestiaClient::new(celestia_url, None)
        .await
        .expect("Failed to connect to Celestia RPC");

    TestEnv {
        provider,
        blobstream_contract,
        counter_contract,
        celestia_client,
    }
}
