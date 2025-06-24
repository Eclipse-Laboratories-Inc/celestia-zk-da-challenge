use anyhow::{anyhow, Context, Result};
use celestia_rpc::{BlobClient, Client as CelestiaClient, TxConfig};
use celestia_types::{nmt::Namespace, AppVersion, Blob};
use clap::Parser;
use dotenv::dotenv;
use toolkit::{BlobIndex, SpanSequence};
use tracing_subscriber::EnvFilter;

/// Binary to upload index blobs to Celestia mainnet for testing
#[derive(Parser)]
struct CliArgs {
    /// Celestia RPC endpoint URL (use mainnet URL)
    #[arg(long, env = "CELESTIA_RPC_URL")]
    celestia_rpc_url: String,

    /// Celestia node auth token
    #[arg(long, env = "CELESTIA_NODE_AUTH_TOKEN")]
    auth_token: String,

    /// Number of dummy blob entries to include in the index
    #[arg(long, default_value = "5")]
    num_dummy_blobs: u64,

    /// Namespace for the index blob (hex encoded, optional)
    #[arg(long)]
    namespace: Option<String>,
}

fn create_dummy_span_sequence(id: u64) -> SpanSequence {
    // Create dummy span sequences with realistic-looking values
    // These would normally reference real blobs, but for testing we use dummy values
    SpanSequence {
        height: 1000000 + id * 100, // Dummy block heights
        start: (id * 1000) as u32,  // Dummy start indices
        size: 32 + (id % 10) as u32, // Varying sizes between 32-41
    }
}

async fn create_and_upload_index_blob(
    client: &CelestiaClient,
    namespace: Namespace,
    num_dummy_blobs: u64,
) -> Result<u64> {
    // Create a new BlobIndex with dummy data
    let mut blob_spans = Vec::new();
    
    println!("Creating index blob with {} dummy entries:", num_dummy_blobs);
    
    // Add dummy blob entries
    for i in 1..=num_dummy_blobs {
        let span = create_dummy_span_sequence(i);
        
        println!(
            "  Entry {}: height={}, start={}, size={}",
            i,
            span.height,
            span.start,
            span.size
        );
        
        blob_spans.push(span);
    }

    let blob_index = BlobIndex {
        blobs: blob_spans,
    };

    // Serialize the blob index
    let serialized_index = bincode::serialize(&blob_index)
        .context("Failed to serialize blob index")?;

    println!("Serialized index blob size: {} bytes", serialized_index.len());

    // Create a Celestia blob with the serialized index
    let celestia_blob = Blob::new(namespace, serialized_index, AppVersion::V2)
        .context("Failed to create Celestia blob")?;

    println!("Submitting index blob to Celestia...");

    // Submit the blob to Celestia
    match client.blob_submit(&[celestia_blob], TxConfig::default()).await {
        Ok(height) => {
            println!("✅ Successfully uploaded index blob!");
            println!("   Block height: {}", height);
            Ok(height)
        }
        Err(e) => {
            println!("❌ Failed to upload index blob: {}", e);
            println!("   Error details: {:?}", e);
            Err(anyhow!("Failed to submit blob to Celestia: {}", e))
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Parse command line arguments
    let args = CliArgs::try_parse()?;

    // Create Celestia client
    let client = CelestiaClient::new(&args.celestia_rpc_url, Some(&args.auth_token))
        .await
        .context("Failed to create Celestia RPC client")?;

    // Create or parse namespace
    let namespace = if let Some(ns_hex) = args.namespace {
        let ns_bytes = hex::decode(ns_hex)
            .context("Invalid hex namespace")?;
        if ns_bytes.len() <= 10 {
            // User provided a suffix (<=10 bytes), create a proper v0 namespace
            let mut full_id = [0u8; 28];
            // First 18 bytes must be zero for v0 namespace
            // Copy the user suffix to the end (bytes 18-27)
            let suffix_start = 28 - ns_bytes.len();
            full_id[suffix_start..].copy_from_slice(&ns_bytes);
            Namespace::new_v0(&full_id)
                .context("Failed to create namespace")?
        } else if ns_bytes.len() == 28 {
            // User provided full 28-byte namespace
            Namespace::new_v0(&ns_bytes)
                .context("Failed to create namespace")?
        } else {
            return Err(anyhow!("Namespace must be <=10 bytes (suffix) or exactly 28 bytes (full ID)"));
        }
    } else {
        // Create a test namespace different from production "636C69" (cli)
        // Use "test01" as our suffix
        let test_suffix = b"test01";
        let mut full_id = [0u8; 28];
        // First 18 bytes are zero (required for v0)
        // Copy "test01" to the end
        let suffix_start = 28 - test_suffix.len();
        full_id[suffix_start..].copy_from_slice(test_suffix);
        
        Namespace::new_v0(&full_id)
            .context("Failed to create test namespace")?
    };

    println!("Using namespace: {}", hex::encode(namespace.as_bytes()));

    // Create and upload the index blob
    match create_and_upload_index_blob(&client, namespace, args.num_dummy_blobs).await {
        Ok(height) => {
            println!("\n🎉 Index blob upload completed successfully!");
            println!("   Block height: {}", height);
            println!("   Namespace: {}", hex::encode(namespace.as_bytes()));
            println!("\nYou can verify the blob submission on Celestia block explorer at height {}", height);
        }
        Err(e) => {
            eprintln!("❌ Failed to upload index blob: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
} 