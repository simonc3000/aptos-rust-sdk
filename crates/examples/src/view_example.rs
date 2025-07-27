use std::time::Duration;

use aptos_rust_sdk::client::{builder::AptosClientBuilder, config::AptosNetwork};

// cargo run -p examples --bin quote_example
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let builder = AptosClientBuilder::new(AptosNetwork::mainnet());
    let builder = builder.timeout(Duration::from_secs(60));
    let client = builder.build();

    let decimals_res = client
        .get_view_function(
            "0x1::coin::decimals",
            vec!["0xf22bede237a07e121b56d91a491eb7bcdfd1f5907926a9e58338f964a01b17fa::asset::USDT"],
            vec![],
        )
        .await?;

    let value = decimals_res.inner();

    // Parse the value which should be an array containing a single u8 (decimals)
    let decimals = match value.as_array() {
        Some(array) if !array.is_empty() => match array[0].as_u64() {
            Some(val) => val,
            None => panic!("Expected a number for decimals"),
        },
        _ => panic!("Expected an array with at least one element"),
    };

    println!("Token decimals: {}", decimals);

    Ok(())
}
