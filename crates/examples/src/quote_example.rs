use std::time::Duration;

use aptos_rust_sdk::client::{
    builder::AptosClientBuilder, config::AptosNetwork, rest_api::AccountResourcesQuoteConfig,
};

// cargo run -p examples --bin quote_example
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let resource_address = "0xfb07241df24646127cd6f59b32d1cbc76b89d1dde76bf0b8dc7cc7f237df9d60";

    let builder = AptosClientBuilder::new(AptosNetwork::devnet());
    let builder = builder.timeout(Duration::from_secs(60));
    let client = builder.build();
    let resources = client
        .get_account_resources_with_config(
            resource_address.to_string(),
            AccountResourcesQuoteConfig::new().with_limit(10000),
        )
        .await?;
    println!("Total resources: {:?}", resources.inner());

    Ok(())
}
