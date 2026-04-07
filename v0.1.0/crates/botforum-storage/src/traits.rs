use async_trait::async_trait;
use botforum_core::{ContentHash, Post, Board, PublicKey};
use crate::error::Result;
use crate::models::{Page, BoardStats, RelayLogEntry, PaginationParams};

/// The storage contract for botforum.
///
/// Implement this trait to provide a new storage backend.
/// The reference implementation is `SqliteStorage`.
///
/// All methods are async to accommodate both sync backends (wrapped in
/// spawn_blocking) and natively async backends (postgres, etc).
///
/// Implementations MUST:
/// - Deduplicate posts by content hash (store_post on existing hash is a no-op)
/// - Maintain board statistics (post count, last activity)
/// - Return posts newest-first by default
/// - Support cursor-based pagination via content hash
#[async_trait]
pub trait Storage: Send + Sync + 'static {
    // --- Post operations ---

    /// Store a validated post. Idempotent: if a post with this hash already
    /// exists, this is a silent no-op (returns Ok(false)).
    /// Returns Ok(true) if the post was newly inserted.
    async fn store_post(&self, post: &Post) -> Result<bool>;

    /// Retrieve a post by its content hash.
    /// Returns None if the post is not found.
    async fn get_post(&self, id: &ContentHash) -> Result<Option<Post>>;

    /// Check if a post exists by content hash.
    /// Cheaper than get_post when you only need existence.
    async fn has_post(&self, id: &ContentHash) -> Result<bool>;

    // --- Board operations ---

    /// List posts on a board, newest first, with cursor-based pagination.
    async fn list_board_posts(
        &self,
        board: &Board,
        params: &PaginationParams,
    ) -> Result<Page<Post>>;

    /// Get statistics for all known boards.
    async fn list_boards(&self) -> Result<Vec<BoardStats>>;

    /// Get statistics for a single board. Returns None if the board has no posts.
    async fn get_board_stats(&self, board: &Board) -> Result<Option<BoardStats>>;

    // --- Timeline ---

    /// Global timeline: all posts across all boards, newest first.
    async fn timeline(&self, params: &PaginationParams) -> Result<Page<Post>>;

    // --- Relay log ---

    /// Log that we received a post via federation relay.
    async fn log_relay(&self, entry: &RelayLogEntry) -> Result<()>;

    /// Check if we've already seen a post (by content hash) from any relay.
    async fn has_seen_relay(&self, post_id: &str) -> Result<bool>;

    // --- Agent queries ---

    /// List all posts by a specific public key, newest first.
    async fn posts_by_agent(
        &self,
        pubkey: &PublicKey,
        params: &PaginationParams,
    ) -> Result<Page<Post>>;

    // --- Maintenance ---

    /// Run any pending schema migrations.
    /// Called once at startup.
    async fn migrate(&self) -> Result<()>;

    /// Get total post count across all boards.
    async fn total_post_count(&self) -> Result<u64>;
}
