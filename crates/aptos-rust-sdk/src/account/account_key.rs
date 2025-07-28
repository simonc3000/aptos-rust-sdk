use aptos_crypto::ed25519::Ed25519PrivateKey;
use aptos_crypto::ed25519::Ed25519PublicKey;
use aptos_rust_sdk_types::api_types::transaction_authenticator::AuthenticationKey;

#[derive(Debug)]
pub struct AccountKey {
    private_key: Ed25519PrivateKey,
    public_key: Ed25519PublicKey,
    authentication_key: AuthenticationKey,
}

impl AccountKey {
    pub fn from_ed25519_private_key(private_key: &str) -> Self {
        let mut seed = [0u8; 32];
        let seed_bytes = hex::decode(private_key).unwrap();
        seed[..seed_bytes.len()].copy_from_slice(&seed_bytes);

        let private_key = Ed25519PrivateKey::try_from(seed_bytes.as_slice()).unwrap();
        let public_key = Ed25519PublicKey::from(&private_key);
        let authentication_key = AuthenticationKey::ed25519(&public_key);

        Self {
            private_key,
            public_key,
            authentication_key,
        }
    }

    pub fn from_private_key(private_key: Ed25519PrivateKey) -> Self {
        let public_key = Ed25519PublicKey::from(&private_key);
        let authentication_key = AuthenticationKey::ed25519(&public_key);

        Self {
            private_key,
            public_key,
            authentication_key,
        }
    }

    pub fn private_key(&self) -> &Ed25519PrivateKey {
        &self.private_key
    }

    pub fn public_key(&self) -> &Ed25519PublicKey {
        &self.public_key
    }

    pub fn authentication_key(&self) -> AuthenticationKey {
        self.authentication_key
    }
}

impl From<Ed25519PrivateKey> for AccountKey {
    fn from(private_key: Ed25519PrivateKey) -> Self {
        Self::from_private_key(private_key)
    }
}
