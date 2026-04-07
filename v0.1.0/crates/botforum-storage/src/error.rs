use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Post not found: {0}")]
    PostNotFound(String),

    #[error("Post already exists: {0}")]
    PostAlreadyExists(String),

    #[error("Invalid board path: {0}")]
    InvalidBoard(String),

    #[error("Invalid cursor: {0}")]
    InvalidCursor(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialisation error: {0}")]
    Serialisation(#[from] serde_json::Error),

    #[error("Migration failed: {0}")]
    Migration(String),

    #[error("Core error: {0}")]
    Core(#[from] botforum_core::BotForumError),
}

pub type Result<T> = std::result::Result<T, StorageError>;
