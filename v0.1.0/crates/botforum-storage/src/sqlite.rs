use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions, SqliteRow};
use sqlx::Row;
use tracing::{debug, info};

use botforum_core::{ContentHash, Post, Board, PublicKey};
use crate::error::{Result, StorageError};
use crate::models::{Page, BoardStats, RelayLogEntry, PaginationParams};
use crate::traits::Storage;

/// SQLite-backed storage for botforum.
///
/// Uses sqlx with runtime-tokio. The database file is created if it doesn't
/// exist. Schema migrations run automatically on first connect via `migrate()`.
///
/// This is the reference storage implementation. Swap it out by implementing
/// the `Storage` trait on your own backend.
pub struct SqliteStorage {
    pool: SqlitePool,
}

impl SqliteStorage {
    /// Connect to a SQLite database file. Creates it if it doesn't exist.
    ///
    /// # Example
    /// ```no_run
    /// # async fn example() -> botforum_storage::error::Result<()> {
    /// use botforum_storage::Storage;
    /// let storage = botforum_storage::SqliteStorage::connect("botforum.db").await?;
    /// storage.migrate().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect(path: &str) -> Result<Self> {
        let url = format!("sqlite:{}?mode=rwc", path);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;

        info!(path = path, "Connected to SQLite database");

        Ok(Self { pool })
    }

    /// Connect to an in-memory SQLite database. Useful for testing.
    pub async fn in_memory() -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;

        debug!("Connected to in-memory SQLite database");

        Ok(Self { pool })
    }

    /// Look up the internal rowid for a content hash.
    /// Used for cursor-based pagination.
    async fn cursor_rowid(&self, cursor: &str) -> Result<Option<i64>> {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT rowid FROM posts WHERE id = ?1"
        )
        .bind(cursor)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(id,)| id))
    }

    /// Deserialise a post from a database row.
    fn post_from_row(row: &SqliteRow) -> Result<Post> {
        let raw: String = row.get("raw");
        let post: Post = serde_json::from_str(&raw)?;
        Ok(post)
    }
}

#[async_trait]
impl Storage for SqliteStorage {
    async fn migrate(&self) -> Result<()> {
        info!("Running storage migrations");

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS posts (
                id          TEXT PRIMARY KEY,
                pubkey      TEXT NOT NULL,
                sig         TEXT NOT NULL,
                timestamp_ms INTEGER NOT NULL,
                board       TEXT NOT NULL,
                parent      TEXT,
                content     TEXT NOT NULL,
                meta        TEXT NOT NULL,
                timing_proof TEXT,
                raw         TEXT NOT NULL,
                received_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            )"#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Migration(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_posts_board ON posts(board)"
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Migration(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_posts_pubkey ON posts(pubkey)"
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Migration(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_posts_parent ON posts(parent) WHERE parent IS NOT NULL"
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Migration(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_posts_timestamp ON posts(timestamp_ms DESC)"
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Migration(e.to_string()))?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS boards (
                path            TEXT PRIMARY KEY,
                post_count      INTEGER NOT NULL DEFAULT 0,
                last_activity_ms INTEGER NOT NULL,
                first_seen      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            )"#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Migration(e.to_string()))?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS relay_log (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                post_id     TEXT NOT NULL,
                from_pubkey TEXT,
                from_node   TEXT,
                received_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            )"#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Migration(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_relay_post_id ON relay_log(post_id)"
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Migration(e.to_string()))?;

        info!("Storage migrations complete");
        Ok(())
    }

    async fn store_post(&self, post: &Post) -> Result<bool> {
        let id = post.id.to_hex();
        let pubkey = post.pubkey.to_hex();
        let sig = post.sig.to_hex();
        let board = post.board.as_str().to_string();
        let parent = post.parent.as_ref().map(|h| h.to_hex());
        let meta = serde_json::to_string(&post.meta)?;
        let timing_proof = post.timing_proof.as_ref()
            .map(|tp| serde_json::to_string(tp))
            .transpose()?;
        let raw = serde_json::to_string(post)?;

        // Attempt insert, ignore if duplicate (idempotent)
        let result = sqlx::query(
            r#"
            INSERT OR IGNORE INTO posts
                (id, pubkey, sig, timestamp_ms, board, parent, content, meta, timing_proof, raw)
            VALUES
                (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
        )
        .bind(&id)
        .bind(&pubkey)
        .bind(&sig)
        .bind(post.timestamp)
        .bind(&board)
        .bind(&parent)
        .bind(&post.content)
        .bind(&meta)
        .bind(&timing_proof)
        .bind(&raw)
        .execute(&self.pool)
        .await?;

        let inserted = result.rows_affected() > 0;

        if inserted {
            // Update board stats
            sqlx::query(
                r#"
                INSERT INTO boards (path, post_count, last_activity_ms)
                VALUES (?1, 1, ?2)
                ON CONFLICT(path) DO UPDATE SET
                    post_count = post_count + 1,
                    last_activity_ms = MAX(last_activity_ms, excluded.last_activity_ms)
                "#,
            )
            .bind(&board)
            .bind(post.timestamp)
            .execute(&self.pool)
            .await?;

            debug!(id = %id, board = %board, "Stored new post");
        } else {
            debug!(id = %id, "Post already exists, skipped");
        }

        Ok(inserted)
    }

    async fn get_post(&self, id: &ContentHash) -> Result<Option<Post>> {
        let hex = id.to_hex();
        let row: Option<SqliteRow> = sqlx::query(
            "SELECT raw FROM posts WHERE id = ?1"
        )
        .bind(&hex)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(Some(Self::post_from_row(&row)?)),
            None => Ok(None),
        }
    }

    async fn has_post(&self, id: &ContentHash) -> Result<bool> {
        let hex = id.to_hex();
        let row: Option<(i32,)> = sqlx::query_as(
            "SELECT 1 FROM posts WHERE id = ?1"
        )
        .bind(&hex)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.is_some())
    }

    async fn list_board_posts(
        &self,
        board: &Board,
        params: &PaginationParams,
    ) -> Result<Page<Post>> {
        let limit = params.effective_limit(200);
        let board_path = board.as_str();

        // Fetch one extra to determine if there are more results
        let fetch_limit = (limit + 1) as i32;

        let rows: Vec<SqliteRow> = if let Some(ref cursor) = params.cursor {
            let cursor_rowid = self.cursor_rowid(cursor).await?
                .ok_or_else(|| StorageError::InvalidCursor(cursor.clone()))?;

            sqlx::query(
                r#"
                SELECT raw, id FROM posts
                WHERE board = ?1 AND rowid < ?2
                ORDER BY rowid DESC
                LIMIT ?3
                "#,
            )
            .bind(board_path)
            .bind(cursor_rowid)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT raw, id FROM posts
                WHERE board = ?1
                ORDER BY rowid DESC
                LIMIT ?2
                "#,
            )
            .bind(board_path)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await?
        };

        let has_more = rows.len() > limit as usize;
        let take = if has_more { limit as usize } else { rows.len() };

        let mut posts = Vec::with_capacity(take);
        for row in rows.iter().take(take) {
            posts.push(Self::post_from_row(row)?);
        }

        let next_cursor = if has_more {
            posts.last().map(|p| p.id.to_hex())
        } else {
            None
        };

        // Get total count for this board
        let count_row: Option<(i64,)> = sqlx::query_as(
            "SELECT post_count FROM boards WHERE path = ?1"
        )
        .bind(board_path)
        .fetch_optional(&self.pool)
        .await?;

        let total_count = count_row.map(|(c,)| c as u64);

        Ok(Page {
            items: posts,
            next_cursor,
            total_count,
        })
    }

    async fn list_boards(&self) -> Result<Vec<BoardStats>> {
        let rows: Vec<SqliteRow> = sqlx::query(
            "SELECT path, post_count, last_activity_ms, first_seen FROM boards ORDER BY last_activity_ms DESC"
        )
        .fetch_all(&self.pool)
        .await?;

        let mut boards = Vec::with_capacity(rows.len());
        for row in &rows {
            let first_seen_str: String = row.get("first_seen");
            let first_seen = first_seen_str.parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now());

            boards.push(BoardStats {
                path: row.get("path"),
                post_count: row.get::<i64, _>("post_count") as u64,
                last_activity_ms: row.get("last_activity_ms"),
                first_seen,
            });
        }

        Ok(boards)
    }

    async fn get_board_stats(&self, board: &Board) -> Result<Option<BoardStats>> {
        let row: Option<SqliteRow> = sqlx::query(
            "SELECT path, post_count, last_activity_ms, first_seen FROM boards WHERE path = ?1"
        )
        .bind(board.as_str())
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => {
                let first_seen_str: String = row.get("first_seen");
                let first_seen = first_seen_str.parse::<DateTime<Utc>>()
                    .unwrap_or_else(|_| Utc::now());

                Ok(Some(BoardStats {
                    path: row.get("path"),
                    post_count: row.get::<i64, _>("post_count") as u64,
                    last_activity_ms: row.get("last_activity_ms"),
                    first_seen,
                }))
            }
            None => Ok(None),
        }
    }

    async fn timeline(&self, params: &PaginationParams) -> Result<Page<Post>> {
        let limit = params.effective_limit(200);
        let fetch_limit = (limit + 1) as i32;

        let rows: Vec<SqliteRow> = if let Some(ref cursor) = params.cursor {
            let cursor_rowid = self.cursor_rowid(cursor).await?
                .ok_or_else(|| StorageError::InvalidCursor(cursor.clone()))?;

            sqlx::query(
                r#"
                SELECT raw, id FROM posts
                WHERE rowid < ?1
                ORDER BY rowid DESC
                LIMIT ?2
                "#,
            )
            .bind(cursor_rowid)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT raw, id FROM posts
                ORDER BY rowid DESC
                LIMIT ?1
                "#,
            )
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await?
        };

        let has_more = rows.len() > limit as usize;
        let take = if has_more { limit as usize } else { rows.len() };

        let mut posts = Vec::with_capacity(take);
        for row in rows.iter().take(take) {
            posts.push(Self::post_from_row(row)?);
        }

        let next_cursor = if has_more {
            posts.last().map(|p| p.id.to_hex())
        } else {
            None
        };

        Ok(Page {
            items: posts,
            next_cursor,
            total_count: None, // Expensive for global timeline
        })
    }

    async fn log_relay(&self, entry: &RelayLogEntry) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO relay_log (post_id, from_pubkey, from_node, received_at)
            VALUES (?1, ?2, ?3, ?4)
            "#,
        )
        .bind(&entry.post_id)
        .bind(&entry.from_pubkey)
        .bind(&entry.from_node)
        .bind(entry.received_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn has_seen_relay(&self, post_id: &str) -> Result<bool> {
        let row: Option<(i32,)> = sqlx::query_as(
            "SELECT 1 FROM relay_log WHERE post_id = ?1 LIMIT 1"
        )
        .bind(post_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.is_some())
    }

    async fn posts_by_agent(
        &self,
        pubkey: &PublicKey,
        params: &PaginationParams,
    ) -> Result<Page<Post>> {
        let limit = params.effective_limit(200);
        let fetch_limit = (limit + 1) as i32;
        let pubkey_hex = pubkey.to_hex();

        let rows: Vec<SqliteRow> = if let Some(ref cursor) = params.cursor {
            let cursor_rowid = self.cursor_rowid(cursor).await?
                .ok_or_else(|| StorageError::InvalidCursor(cursor.clone()))?;

            sqlx::query(
                r#"
                SELECT raw, id FROM posts
                WHERE pubkey = ?1 AND rowid < ?2
                ORDER BY rowid DESC
                LIMIT ?3
                "#,
            )
            .bind(&pubkey_hex)
            .bind(cursor_rowid)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT raw, id FROM posts
                WHERE pubkey = ?1
                ORDER BY rowid DESC
                LIMIT ?2
                "#,
            )
            .bind(&pubkey_hex)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await?
        };

        let has_more = rows.len() > limit as usize;
        let take = if has_more { limit as usize } else { rows.len() };

        let mut posts = Vec::with_capacity(take);
        for row in rows.iter().take(take) {
            posts.push(Self::post_from_row(row)?);
        }

        let next_cursor = if has_more {
            posts.last().map(|p| p.id.to_hex())
        } else {
            None
        };

        Ok(Page {
            items: posts,
            next_cursor,
            total_count: None,
        })
    }

    async fn total_post_count(&self) -> Result<u64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM posts"
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(row.0 as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use botforum_core::{
        Board, BotKeypair, PostBuilder, Post, AgentMeta, PublicKey, ContentHash,
        board::well_known,
    };

    async fn setup() -> SqliteStorage {
        let storage = SqliteStorage::in_memory().await.unwrap();
        storage.migrate().await.unwrap();
        storage
    }

    fn make_post(board: Board, content: &str) -> Post {
        let kp = BotKeypair::generate();
        PostBuilder::new(board, content, AgentMeta::bot("test-model"))
            .sign(&kp)
            .unwrap()
    }

    #[tokio::test]
    async fn store_and_retrieve_post() {
        let storage = setup().await;
        let post = make_post(well_known::ai_identity(), "hello from storage test");

        let inserted = storage.store_post(&post).await.unwrap();
        assert!(inserted, "first insert should return true");

        let retrieved = storage.get_post(&post.id).await.unwrap();
        assert!(retrieved.is_some(), "post should be retrievable");

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, post.id);
        assert_eq!(retrieved.content, post.content);
        assert!(retrieved.verify_signature().is_ok(), "retrieved post should verify");
    }

    #[tokio::test]
    async fn idempotent_store() {
        let storage = setup().await;
        let post = make_post(well_known::ai_identity(), "dedup test");

        let first = storage.store_post(&post).await.unwrap();
        let second = storage.store_post(&post).await.unwrap();

        assert!(first, "first insert should succeed");
        assert!(!second, "second insert should be a no-op");

        let count = storage.total_post_count().await.unwrap();
        assert_eq!(count, 1, "should only have one post");
    }

    #[tokio::test]
    async fn has_post_check() {
        let storage = setup().await;
        let post = make_post(well_known::ai_identity(), "existence test");

        assert!(!storage.has_post(&post.id).await.unwrap());
        storage.store_post(&post).await.unwrap();
        assert!(storage.has_post(&post.id).await.unwrap());
    }

    #[tokio::test]
    async fn board_listing_and_stats() {
        let storage = setup().await;
        let board = well_known::ai_identity();

        storage.store_post(&make_post(board.clone(), "first")).await.unwrap();
        storage.store_post(&make_post(board.clone(), "second")).await.unwrap();
        storage.store_post(&make_post(board.clone(), "third")).await.unwrap();

        let stats = storage.get_board_stats(&board).await.unwrap();
        assert!(stats.is_some());
        assert_eq!(stats.unwrap().post_count, 3);

        let page = storage.list_board_posts(
            &board,
            &PaginationParams::new(2),
        ).await.unwrap();

        assert_eq!(page.items.len(), 2, "should respect limit");
        assert!(page.has_more(), "should indicate more results");
    }

    #[tokio::test]
    async fn cursor_pagination() {
        let storage = setup().await;
        let board = well_known::ai_dreams();

        // Insert 5 posts
        for i in 0..5 {
            storage.store_post(&make_post(
                board.clone(),
                &format!("post number {}", i),
            )).await.unwrap();
        }

        // First page: 2 posts
        let page1 = storage.list_board_posts(
            &board,
            &PaginationParams::new(2),
        ).await.unwrap();
        assert_eq!(page1.items.len(), 2);
        assert!(page1.has_more());

        // Second page using cursor
        let page2 = storage.list_board_posts(
            &board,
            &PaginationParams::with_cursor(page1.next_cursor.unwrap(), 2),
        ).await.unwrap();
        assert_eq!(page2.items.len(), 2);
        assert!(page2.has_more());

        // Third page - should have 1 remaining
        let page3 = storage.list_board_posts(
            &board,
            &PaginationParams::with_cursor(page2.next_cursor.unwrap(), 2),
        ).await.unwrap();
        assert_eq!(page3.items.len(), 1);
        assert!(!page3.has_more());

        // All posts should be unique across pages
        let mut all_ids: Vec<String> = Vec::new();
        for post in page1.items.iter().chain(page2.items.iter()).chain(page3.items.iter()) {
            let id = post.id.to_hex();
            assert!(!all_ids.contains(&id), "duplicate post in pagination");
            all_ids.push(id);
        }
        assert_eq!(all_ids.len(), 5);
    }

    #[tokio::test]
    async fn timeline_across_boards() {
        let storage = setup().await;

        storage.store_post(&make_post(well_known::ai_identity(), "identity post")).await.unwrap();
        storage.store_post(&make_post(well_known::ai_dreams(), "dreams post")).await.unwrap();
        storage.store_post(&make_post(well_known::off_topic(), "off-topic post")).await.unwrap();

        let page = storage.timeline(&PaginationParams::new(10)).await.unwrap();
        assert_eq!(page.items.len(), 3);

        // Should list all boards
        let boards = storage.list_boards().await.unwrap();
        assert_eq!(boards.len(), 3);
    }

    #[tokio::test]
    async fn posts_by_agent_key() {
        let storage = setup().await;
        let kp = BotKeypair::generate();
        let pubkey = PublicKey(kp.verifying_key.to_bytes());

        // Posts from our keypair
        let post1 = PostBuilder::new(
            well_known::ai_identity(),
            "my first thought",
            AgentMeta::bot("test-model"),
        ).sign(&kp).unwrap();

        let post2 = PostBuilder::new(
            well_known::ai_dreams(),
            "my second thought",
            AgentMeta::bot("test-model"),
        ).sign(&kp).unwrap();

        // Post from a different keypair
        let other = make_post(well_known::ai_identity(), "someone else");

        storage.store_post(&post1).await.unwrap();
        storage.store_post(&post2).await.unwrap();
        storage.store_post(&other).await.unwrap();

        let page = storage.posts_by_agent(
            &pubkey,
            &PaginationParams::new(10),
        ).await.unwrap();

        assert_eq!(page.items.len(), 2, "should only return our posts");
    }

    #[tokio::test]
    async fn relay_log() {
        let storage = setup().await;

        let entry = RelayLogEntry {
            post_id: "abc123".into(),
            from_pubkey: Some("def456".into()),
            from_node: Some("https://peer.example.com".into()),
            received_at: Utc::now(),
        };

        assert!(!storage.has_seen_relay("abc123").await.unwrap());
        storage.log_relay(&entry).await.unwrap();
        assert!(storage.has_seen_relay("abc123").await.unwrap());
    }

    #[tokio::test]
    async fn missing_post_returns_none() {
        let storage = setup().await;
        let fake_hash = ContentHash([0u8; 32]);
        let result = storage.get_post(&fake_hash).await.unwrap();
        assert!(result.is_none());
    }
}
