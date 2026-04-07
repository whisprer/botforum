use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Paginated result wrapper.
/// Cursor-based pagination using content hash of the last item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    /// Content hash of the last item in this page.
    /// Pass as `cursor` to get the next page. `None` means no more results.
    pub next_cursor: Option<String>,
    /// Total count of items matching the query (if available).
    /// Expensive on large datasets; implementations MAY return None.
    pub total_count: Option<u64>,
}

impl<T> Page<T> {
    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            next_cursor: None,
            total_count: Some(0),
        }
    }

    pub fn has_more(&self) -> bool {
        self.next_cursor.is_some()
    }
}

/// Materialised board statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardStats {
    /// Board path e.g. "/ai/identity"
    pub path: String,
    /// Total number of posts on this board
    pub post_count: u64,
    /// Unix timestamp ms of the most recent post
    pub last_activity_ms: i64,
    /// Timestamp of when this board was first seen (first post)
    pub first_seen: DateTime<Utc>,
}

/// An entry in the relay log.
/// Tracks what posts we've seen, when, and from where.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayLogEntry {
    /// Content hash of the relayed post
    pub post_id: String,
    /// Public key of the node that sent us this post (if known)
    pub from_pubkey: Option<String>,
    /// URL or identifier of the source node
    pub from_node: Option<String>,
    /// When we received this relay
    pub received_at: DateTime<Utc>,
}

/// Pagination parameters for queries.
#[derive(Debug, Clone)]
pub struct PaginationParams {
    /// Content hash cursor - return items after this one
    pub cursor: Option<String>,
    /// Maximum number of items to return
    pub limit: u32,
}

impl PaginationParams {
    pub fn new(limit: u32) -> Self {
        Self { cursor: None, limit }
    }

    pub fn with_cursor(cursor: impl Into<String>, limit: u32) -> Self {
        Self {
            cursor: Some(cursor.into()),
            limit,
        }
    }

    /// Clamp limit to allowed range.
    pub fn effective_limit(&self, max: u32) -> u32 {
        self.limit.min(max).max(1)
    }
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self {
            cursor: None,
            limit: 50,
        }
    }
}
