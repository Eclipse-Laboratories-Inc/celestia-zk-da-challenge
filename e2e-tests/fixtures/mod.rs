//! Shared fixtures and contract binding for the end-to-end test crate.
//!
//! * Requires the `anvil` binary somewhere in `$PATH` (or `foundryup --bin anvil`).
//! * Uses rstest’s `#[once]` so Anvil and the deployment happen **exactly one time**
//!   per test binary run.

use crate::fixtures::Counter::CounterInstance;
use alloy::network::EthereumWallet;
use alloy::node_bindings::{Anvil, AnvilInstance};
use alloy::providers::{DynProvider, Provider, ProviderBuilder};
use alloy::signers::local::PrivateKeySigner;
use alloy::sol;
use rstest::*;
use std::str::FromStr;

sol!(
    #[sol(rpc)]
    Counter,
    "../out/Counter.sol/Counter.json"
);


pub struct TestEnv {
    pub provider: DynProvider,
    pub counter_contract: CounterInstance<(), DynProvider>,
    _anvil: AnvilInstance,
}

async fn deploy_counter(provider: DynProvider) -> CounterInstance<(), DynProvider> {
    let deployer_address = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
        .parse()
        .expect("Failed to parse deployer address");

    // no async #[once] fixture: create a throw-away Tokio runtime inside the call
    Counter::deploy(provider, deployer_address).await.expect("Failed to deploy Counter")
}

#[fixture]
pub async fn test_env() -> TestEnv {
    // Use Anvil's first default account
    let private_key = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    let signer = PrivateKeySigner::from_str(private_key).unwrap();
    let wallet = EthereumWallet::from(signer);

    let anvil = Anvil::new().block_time(1).chain_id(1337).try_spawn().expect("failed to spawn anvil instance");

    let provider = ProviderBuilder::new()
        .wallet(wallet)
        .on_anvil_with_config(|anvil| anvil.block_time(1).chain_id(1337))
        .erased();

    let counter_contract = deploy_counter(provider.clone()).await;

    TestEnv {
        provider,
        counter_contract,
        _anvil: anvil,
    }
}
