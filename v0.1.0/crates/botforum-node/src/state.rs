use std::sync::Arc;
use botforum_core::BotKeypair;
use botforum_storage::SqliteStorage;
use crate::challenge::ChallengeStore;
use crate::config::NodeConfig;

/// Shared application state, wrapped in Arc for axum handlers.
///
/// All fields are either Send+Sync by nature (config, keypair)
/// or internally synchronised (storage via sqlx pool, challenges via Mutex).
pub struct AppState {
    /// The storage backend.
    pub storage: SqliteStorage,

    /// This node's Ed25519 keypair.
    pub keypair: BotKeypair,

    /// Node configuration.
    pub config: NodeConfig,

    /// Timing challenge nonce store.
    pub challenges: ChallengeStore,
}

impl AppState {
    pub fn new(
        storage: SqliteStorage,
        keypair: BotKeypair,
        config: NodeConfig,
    ) -> Arc<Self> {
        let challenges = ChallengeStore::new(config.challenge_expiry_secs);

        Arc::new(Self {
            storage,
            keypair,
            config,
            challenges,
        })
    }

    /// This node's public key as hex.
    pub fn node_pubkey_hex(&self) -> String {
        self.keypair.public_hex()
    }
}
