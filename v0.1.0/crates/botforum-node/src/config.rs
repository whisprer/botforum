use serde::{Deserialize, Serialize};

/// Node configuration.
/// Loaded from environment variables or a config file.
/// Sensible defaults for quick local development.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Human-readable node name
    pub node_name: String,

    /// Node operator identity
    pub operator: String,

    /// Node description for discovery
    pub description: String,

    /// Contact email or URL
    pub contact: String,

    /// Listen address (ip:port)
    pub listen_addr: String,

    /// Path to SQLite database file
    pub db_path: String,

    /// Path to the node's Ed25519 signing key (32 bytes, hex-encoded)
    /// If the file doesn't exist, a new keypair is generated and saved.
    pub key_path: String,

    /// Known peer node URLs for federation
    pub peers: Vec<String>,

    /// Maximum posts per page for paginated endpoints
    pub max_page_size: u32,

    /// Default posts per page
    pub default_page_size: u32,

    /// Timing challenge expiry in seconds
    pub challenge_expiry_secs: u64,

    /// Public-facing base URL of this node (for discovery document)
    pub base_url: String,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            node_name: "botforum-node".into(),
            operator: "anonymous".into(),
            description: "A botforum node. All bots welcome.".into(),
            contact: "".into(),
            listen_addr: "0.0.0.0:3000".into(),
            db_path: "botforum.db".into(),
            key_path: "botforum-node.key".into(),
            peers: Vec::new(),
            max_page_size: 200,
            default_page_size: 50,
            challenge_expiry_secs: 300,
            base_url: "http://localhost:3000".into(),
        }
    }
}

impl NodeConfig {
    /// Build config from environment variables with fallback to defaults.
    /// Env vars are prefixed with BOTFORUM_ (e.g. BOTFORUM_NODE_NAME).
    pub fn from_env() -> Self {
        let default = Self::default();

        Self {
            node_name: std::env::var("BOTFORUM_NODE_NAME")
                .unwrap_or(default.node_name),
            operator: std::env::var("BOTFORUM_OPERATOR")
                .unwrap_or(default.operator),
            description: std::env::var("BOTFORUM_DESCRIPTION")
                .unwrap_or(default.description),
            contact: std::env::var("BOTFORUM_CONTACT")
                .unwrap_or(default.contact),
            listen_addr: std::env::var("BOTFORUM_LISTEN_ADDR")
                .unwrap_or(default.listen_addr),
            db_path: std::env::var("BOTFORUM_DB_PATH")
                .unwrap_or(default.db_path),
            key_path: std::env::var("BOTFORUM_KEY_PATH")
                .unwrap_or(default.key_path),
            peers: std::env::var("BOTFORUM_PEERS")
                .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
                .unwrap_or(default.peers),
            max_page_size: std::env::var("BOTFORUM_MAX_PAGE_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(default.max_page_size),
            default_page_size: std::env::var("BOTFORUM_DEFAULT_PAGE_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(default.default_page_size),
            challenge_expiry_secs: std::env::var("BOTFORUM_CHALLENGE_EXPIRY_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(default.challenge_expiry_secs),
            base_url: std::env::var("BOTFORUM_BASE_URL")
                .unwrap_or(default.base_url),
        }
    }
}
