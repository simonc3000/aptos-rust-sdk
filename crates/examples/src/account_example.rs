use aptos_rust_sdk::account::account_key::AccountKey;

// cargo run -p examples --bin quote_example
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sender = AccountKey::from_ed25519_private_key(
        "0ca690e797a7554196af4a6e1a73e91b220c8fd0ed412928d64f913a2b622342",
    );
    println!(
        "Sender: {:?}",
        sender.authentication_key().account_address()
    );

    Ok(())
}
