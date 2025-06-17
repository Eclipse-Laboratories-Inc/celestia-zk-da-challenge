use crate::contracts::Blobstream0::Blobstream0Instance;
use alloy::primitives::Address;
use alloy_contract::private::{Provider, Transport};
use futures_util::StreamExt;

/// Parses deployment output to extract verifier and contract addresses.
///
/// # Arguments
/// - `content`: The string containing the output of `blobstream0 deploy`.
///
/// Returns a tuple containing (verifier_address, contract_address) or an error
fn parse_deployment_addresses(
    blobstream0_deploy_output: &str,
) -> Result<(String, String), &'static str> {
    let mut verifier_address = None;
    let mut contract_address = None;

    for line in blobstream0_deploy_output.lines() {
        if line.contains("deployed verifier to address:") {
            let parts: Vec<&str> = line.split("deployed verifier to address:").collect();
            if parts.len() >= 2 {
                verifier_address = Some(parts[1].trim().to_string());
            }
        } else if line.contains("deployed contract to address:") {
            let parts: Vec<&str> = line.split("deployed contract to address:").collect();
            if parts.len() >= 2 {
                contract_address = Some(parts[1].trim().to_string());
            }
        }
    }

    match (verifier_address, contract_address) {
        (Some(v), Some(c)) => Ok((v, c)),
        _ => Err("Failed to find both addresses in the content"),
    }
}

pub fn get_blobstream_address() -> Address {
    let output = std::process::Command::new("docker")
        .args(["exec", "blobstream0-dev", "cat", ".deployed"])
        .output()
        .expect("Failed to retrieve .deployed file content from Docker container");

    if !output.status.success() {
        panic!(
            "Docker command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let blobstream_address = parse_deployment_addresses(&String::from_utf8_lossy(&output.stdout))
        .expect("Failed to parse deployment output")
        .1;

    Address::parse_checksummed(&blobstream_address, None).expect("Failed to parse address")
}

const BLOBSTREAM_BATCH_SIZE: u64 = 4;

pub async fn wait_for_blobstream_inclusion<
    T: Clone + Transport,
    P: Provider<T, alloy::network::Ethereum>,
>(
    blobstream_contract: &Blobstream0Instance<T, P>,
    target_height: u64,
) -> anyhow::Result<()> {
    let current_eth_block = blobstream_contract.provider().get_block_number().await?;

    // Sometimes Anvil does not return the data from the RPC despite sending us the corresponding
    // event, so we add a margin of one Blobstream batch size to be safe.
    // TODO: determine what's causing this timing issue between event and RPC data availability.
    let target_height = target_height + BLOBSTREAM_BATCH_SIZE;

    let current: u64 = blobstream_contract.latestHeight().call().await?._0;
    println!("Current Blobstream height: {current}");
    if current >= target_height {
        return Ok(());
    }

    let mut event_stream = blobstream_contract
        .HeadUpdate_filter()
        .from_block(current_eth_block) // block number or tag
        .watch() // ↳ yields `HeaderSynced` structs
        .await?
        .into_stream();

    while let Some(evt) = event_stream.next().await {
        let evt = evt?; // unwrap provider errors
        println!("Blobstream head update: {}", evt.0.blockNumber);

        if evt.0.blockNumber >= target_height {
            return Ok(());
        }
    }

    // Sub-stream ended unexpectedly (provider closed) - treat as error.
    Err(anyhow::anyhow!("event stream closed before height reached"))
}

pub async fn wait_for_blobstream_inclusion_with_timeout<T, P>(
    blobstream_contract: &Blobstream0Instance<T, P>,
    target_height: u64,
    timeout: std::time::Duration,
) -> anyhow::Result<()>
where
    T: Clone + Transport,
    P: Provider<T, alloy::network::Ethereum>,
{
    match tokio::time::timeout(
        timeout,
        wait_for_blobstream_inclusion(blobstream_contract, target_height),
    )
    .await
    {
        Ok(res) => res, // completed in time
        Err(_) => Err(anyhow::anyhow!(
            "timed out before target height ({}) was reached",
            target_height
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_deployment_addresses() {
        let content = "deployed verifier to address: 0x959922bE3CAee4b8Cd9a407cc3ac1C251C2007B1\ndeployed contract to address: 0x68B1D87F95878fE05B998F19b66F4baba5De1aed";

        let result = parse_deployment_addresses(content).unwrap();
        assert_eq!(result.0, "0x959922bE3CAee4b8Cd9a407cc3ac1C251C2007B1");
        assert_eq!(result.1, "0x68B1D87F95878fE05B998F19b66F4baba5De1aed");
    }
}
