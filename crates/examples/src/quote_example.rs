use std::time::Duration;

use aptos_rust_sdk::client::{
    builder::AptosClientBuilder, config::AptosNetwork, rest_api::AccountResourcesQuoteConfig,
};
use serde_json::Value;

// cargo run -p examples --bin quote_example
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let resource_address = "0x48271d39d0b05bd6efca2278f22277d6fcc375504f9839fd73f74ace240861af";

    let builder = AptosClientBuilder::new(AptosNetwork::mainnet());
    let builder = builder.timeout(Duration::from_secs(60));
    let client = builder.build();
    let resources = client
        .get_account_resources_with_config(
            resource_address.to_string(),
            AccountResourcesQuoteConfig::new().with_limit(10000),
        )
        .await?;
    println!("Total resources: {}", resources.inner().len());

    let pool_resources: Vec<&_> = resources
        .inner()
        .iter()
        .filter(|resource| {
            let resource_type = &resource.type_;
            resource_type.starts_with(&format!(
                "{}::weighted_pool::WeightedPool<",
                resource_address
            )) || resource_type
                .starts_with(&format!("{}::stable_pool::StablePool<", resource_address))
        })
        .collect();
    println!("Found {} pool resources", pool_resources.len());

    for (index, resource) in pool_resources.iter().enumerate() {
        println!("\nPool Resource {}:", index + 1);
        println!("Type: {}", resource.type_);

        // Extract pool data
        if let Some(data) = resource.data.as_object() {
            // Extract asset values
            if let Some(asset_0) = extract_asset_value(data, "asset_0") {
                println!("Asset 0: {}", asset_0);
            }
            if let Some(asset_1) = extract_asset_value(data, "asset_1") {
                println!("Asset 1: {}", asset_1);
            }
            if let Some(asset_2) = extract_asset_value(data, "asset_2") {
                println!("Asset 2: {}", asset_2);
            }
            if let Some(asset_3) = extract_asset_value(data, "asset_3") {
                println!("Asset 3: {}", asset_3);
            }

            // Extract amp_factor for stable pools
            if let Some(amp_factor) = data.get("amp_factor").and_then(|v| v.as_str()) {
                println!("Amp Factor: {}", amp_factor);
            }

            // Extract swap fee ratio
            if let Some(swap_fee) = extract_swap_fee_ratio(data) {
                println!("Swap Fee Ratio: {}", swap_fee);
            }
        }
    }

    // Create a structured representation similar to the TypeScript interface
    let structured_pools: Vec<PoolResource> = pool_resources
        .iter()
        .filter_map(|resource| {
            let data = resource.data.as_object()?;
            Some(PoolResource {
                resource_type: resource.type_.clone(),
                asset_0: extract_asset_value(data, "asset_0")?,
                asset_1: extract_asset_value(data, "asset_1")?,
                asset_2: extract_asset_value(data, "asset_2"),
                asset_3: extract_asset_value(data, "asset_3"),
                amp_factor: data
                    .get("amp_factor")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                swap_fee_ratio: extract_swap_fee_ratio(data)?,
            })
        })
        .collect();

    println!("\nStructured Pool Resources:");
    for pool in &structured_pools {
        println!("{:#?}", pool);
    }

    Ok(())
}

// Helper function to extract asset value
fn extract_asset_value(data: &serde_json::Map<String, Value>, asset_key: &str) -> Option<String> {
    data.get(asset_key)?
        .as_object()?
        .get("value")?
        .as_str()
        .map(String::from)
}

// Helper function to extract swap fee ratio
fn extract_swap_fee_ratio(data: &serde_json::Map<String, Value>) -> Option<String> {
    data.get("swap_fee_ratio")?
        .as_object()?
        .get("v")?
        .as_str()
        .map(String::from)
}

// Structured representation of pool resource
#[derive(Debug, Clone)]
struct PoolResource {
    pub resource_type: String,
    pub asset_0: String,
    pub asset_1: String,
    pub asset_2: Option<String>,
    pub asset_3: Option<String>,
    pub amp_factor: Option<String>,
    pub swap_fee_ratio: String,
}
