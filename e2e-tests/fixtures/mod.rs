//! Shared fixtures and contract binding for the end-to-end test crate.
//!
//! * Requires the `anvil` binary somewhere in `$PATH` (or `foundryup --bin anvil`).
//! * Uses rstest’s `#[once]` so Anvil and the deployment happen **exactly one time**
//!   per test binary run.

use std::str::FromStr;
use alloy::network::EthereumWallet;
use crate::fixtures::SimpleStorage::SimpleStorageInstance;
use alloy::providers::{DynProvider, Provider, ProviderBuilder};
use alloy::signers::local::{PrivateKeySigner};
use alloy::sol;
use once_cell::sync::Lazy;
use rstest::*;
use tokio::sync::OnceCell;

/// Spins up an Anvil child-process once and keeps it for the whole run.
///
/// The helper automatically unlocks Anvil’s first dev account and wires it as
/// the signer for the returned provider.
#[fixture]
#[once]
pub fn provider() -> &'static DynProvider {
    static INSTANCE: Lazy<DynProvider> = Lazy::new(|| {
        // Use Anvil's first default account
        let private_key = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
        let signer = PrivateKeySigner::from_str(private_key).unwrap();
        let wallet = EthereumWallet::from(signer);

        ProviderBuilder::new()
            .wallet(wallet)
            .on_anvil_with_config(|anvil| anvil.block_time(1).chain_id(1337))
            .erased()
    });
    &INSTANCE
}

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

#[fixture]
pub async fn deployed_contract(
    provider: &'static DynProvider,
) -> &'static SimpleStorageInstance<(), DynProvider> {
    // One global cell that will hold the handle
    static CONTRACT: OnceCell<SimpleStorageInstance<(), DynProvider>> = OnceCell::const_new();

    CONTRACT.get_or_try_init(|| {
        // no async #[once] fixture: create a throw-away Tokio runtime inside the call
        SimpleStorage::deploy(provider.clone())
    }).await.expect("Failed to deploy contract")
}
