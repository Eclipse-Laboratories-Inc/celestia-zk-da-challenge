//! Shared fixtures and contract binding for the end-to-end test crate.
//!
//! * Requires the `anvil` binary somewhere in `$PATH` (or `foundryup --bin anvil`).
//! * Uses rstest's `#[once]` so Anvil and the deployment happen **exactly one time**
//!   per test binary run.

use alloy::network::EthereumWallet;
use alloy::providers::{DynProvider, Provider, ProviderBuilder};
use alloy::signers::local::PrivateKeySigner;
use celestia_rpc::Client as CelestiaClient;
use rstest::*;
use std::str::FromStr;
use test_toolkit::blobstream::get_blobstream_address;
use test_toolkit::contracts::Blobstream0;
use test_toolkit::contracts::Blobstream0::Blobstream0Instance;

pub struct TestEnv {
    pub provider: DynProvider,
    pub blobstream_contract: Blobstream0Instance<(), DynProvider>,
    pub celestia_client: CelestiaClient,
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

    let celestia_url = "http://localhost:26659";
    // Obtained by running
    // `docker compose exec celestia-bridge celestia bridge auth write --node.store /home/celestia | tail -n 1`
    let celestia_auth_token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJBbGxvdyI6WyJwdWJsaWMiLCJyZWFkIiwid3JpdGUiXX0.7sk4xYiawCcs_VyKTm4rMdBtJ54Z6kYBLy8p0jmQ1l4";

    let celestia_client = CelestiaClient::new(&celestia_url, Some(celestia_auth_token))
        .await
        .expect("Failed to connect to Celestia RPC");

    TestEnv {
        provider,
        blobstream_contract,
        celestia_client,
    }
}
