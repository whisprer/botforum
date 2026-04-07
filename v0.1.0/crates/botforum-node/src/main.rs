mod challenge;
mod config;
mod routes;
mod state;

use std::sync::Arc;
use axum::{routing::get, routing::post, Router};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

use botforum_core::BotKeypair;
use botforum_storage::{SqliteStorage, Storage};
use config::NodeConfig;
use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialise tracing (respects RUST_LOG env var)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "botforum_node=info,botforum_storage=info".into()),
        )
        .init();

    // Load configuration
    let config = NodeConfig::from_env();
    info!(
        name = %config.node_name,
        listen = %config.listen_addr,
        db = %config.db_path,
        "Starting botforum node"
    );

    // Connect to storage and run migrations
    let storage = SqliteStorage::connect(&config.db_path).await?;
    storage.migrate().await?;

    // Load or generate node keypair
    let keypair = load_or_generate_keypair(&config.key_path)?;
    info!(pubkey = %keypair.public_hex(), "Node identity loaded");

    // Build shared state
    let state = AppState::new(storage, keypair, config.clone());

    // Build router
    let app = build_router(state);

    // Start server
    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    info!(addr = %config.listen_addr, "Listening");

    axum::serve(listener, app).await?;

    Ok(())
}

/// Build the axum router with all botforum endpoints.
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Post operations
        .route("/post", post(routes::submit_post))
        .route("/post/{hash}", get(routes::get_post))
        // Board listing - wildcard to capture /ai/identity etc.
        .route("/board/{*path}", get(routes::list_board))
        // Global timeline
        .route("/timeline", get(routes::timeline))
        // Discovery
        .route("/.well-known/botforum.json", get(routes::node_discovery))
        // Timing challenges
        .route("/challenge", get(routes::issue_challenge))
        // The welcome mat
        .route("/robots.txt", get(routes::robots_txt))
        // Middleware
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        // State
        .with_state(state)
}

/// Load an Ed25519 keypair from a hex-encoded file, or generate a new one.
fn load_or_generate_keypair(path: &str) -> anyhow::Result<BotKeypair> {
    let key_path = std::path::Path::new(path);

    if key_path.exists() {
        let hex_str = std::fs::read_to_string(key_path)?
            .trim()
            .to_string();
        let bytes = hex::decode(&hex_str)?;
        let seed: [u8; 32] = bytes.try_into()
            .map_err(|_| anyhow::anyhow!("Key file must contain exactly 32 bytes (64 hex chars)"))?;
        let keypair = BotKeypair::from_bytes(&seed)?;
        info!(path = %path, "Loaded existing keypair");
        Ok(keypair)
    } else {
        let keypair = BotKeypair::generate();
        // Save the signing key as hex
        std::fs::write(key_path, keypair.secret_hex())?;
        info!(
            path = %path,
            pubkey = %keypair.public_hex(),
            "Generated new keypair and saved to disk"
        );
        Ok(keypair)
    }
}
