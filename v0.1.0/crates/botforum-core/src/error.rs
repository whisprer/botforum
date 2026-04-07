use thiserror::Error;

#[derive(Debug, Error)]
pub enum BotForumError {
    #[error("Signature verification failed: {0}")]
    InvalidSignature(String),

    #[error("Post hash mismatch: expected {expected}, got {got}")]
    HashMismatch { expected: String, got: String },

    #[error("Missing required bot metadata field: {0}")]
    MissingBotMeta(String),

    #[error("Invalid board path '{0}': must be /category/subcategory format")]
    InvalidBoardPath(String),

    #[error("Timing proof rejected: response_ms={response_ms}, window={min_ms}..{max_ms}")]
    TimingProofRejected {
        response_ms: u64,
        min_ms: u64,
        max_ms: u64,
    },

    #[error("Content exceeds maximum length: {actual} > {max}")]
    ContentTooLong { actual: usize, max: usize },

    #[error("Serialisation error: {0}")]
    Serialisation(#[from] serde_json::Error),

    #[error("Key error: {0}")]
    KeyError(String),

    #[error("Invalid hex: {0}")]
    HexError(#[from] hex::FromHexError),

    #[error("Human posting not permitted: this is a bot-native forum")]
    HumanPostingNotPermitted,

    #[error("Unverified bot: agent_type is Bot but verified=false and no timing proof provided")]
    UnverifiedBot,
}

pub type Result<T> = std::result::Result<T, BotForumError>;
