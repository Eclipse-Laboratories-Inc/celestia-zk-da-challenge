//! Shared fixtures and contract binding for the end-to-end test crate.
//!
//! * Requires the `anvil` binary somewhere in `$PATH` (or `foundryup --bin anvil`).
//! * Uses rstest’s `#[once]` so Anvil and the deployment happen **exactly one time**
//!   per test binary run.

use crate::fixtures::Counter::CounterInstance;
use alloy::network::EthereumWallet;
use alloy::providers::{DynProvider, Provider, ProviderBuilder};
use alloy::signers::local::PrivateKeySigner;
use alloy::sol;
use rstest::*;
use std::str::FromStr;
use alloy::node_bindings::{Anvil, AnvilInstance};

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

sol!(
    #[sol(rpc)]
    Counter,
    "../out/Counter.sol/Counter.json"
);

sol! {
    #[sol(
        rpc,
        // ↓ super-minimal byte-code that just stores one uint256 (size ~150 B)
        bytecode = "608060405234801561001057600080fd5b50600160008190555060d7806100286000396000f3fe608060405260043610601c5760003560e01c80632a1afcd91460215780636d4ce63c14602f575b600080fd5b60276049565b6040518082815260200191505060405180910390f35b60356057565b6040518082815260200191505060405180910390f35b60005481565b600160008190555056fea2646970667358221220ac4c6f3dc8e8a3e14decb38f6131aeec12cc3e018e70b22aabca1e42ca7e261564736f6c63430008110033"
    )]
    contract SimpleStorage {
        uint256 public value;

        function set(uint256 newValue) public {
            value = newValue;
        }
    }
}
