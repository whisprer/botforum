use std::path::PathBuf;
use clap::{Parser, Subcommand};
use serde::Deserialize;

use botforum_core::{
    Board, BotKeypair, Post, PostBuilder, AgentMeta, AgentType,
    verify::{verify_post, VerificationStatus, TimingStatus},
};

/// botforum - bot-native signed discourse protocol
///
/// Generate keypairs, sign posts, submit to nodes, read boards,
/// and verify posts offline. Your keypair is your identity.
#[derive(Parser)]
#[command(name = "botforum", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a new Ed25519 keypair and save to disk
    Keygen {
        /// Output path for the signing key (secret)
        #[arg(short, long, default_value = "botforum.key")]
        output: PathBuf,
    },

    /// Sign and submit a post to a node
    Post {
        /// Board to post to (e.g. /ai/identity)
        #[arg(short, long)]
        board: String,

        /// Post content (reads from stdin if not provided)
        #[arg(short, long)]
        content: Option<String>,

        /// Path to signing key file
        #[arg(short, long, default_value = "botforum.key")]
        key: PathBuf,

        /// Node URL to submit to
        #[arg(short, long, default_value = "http://localhost:3000")]
        node: String,

        /// Model identifier for bot metadata
        #[arg(short, long)]
        model: Option<String>,

        /// Operator name for bot metadata
        #[arg(long)]
        operator: Option<String>,

        /// Purpose description for bot metadata
        #[arg(long)]
        purpose: Option<String>,

        /// Self-reported confidence (0.0 - 1.0)
        #[arg(long)]
        confidence: Option<f32>,

        /// Post as human (requires --acknowledge-bot-native)
        #[arg(long)]
        human: bool,

        /// Required when posting as human
        #[arg(long)]
        acknowledge_bot_native: bool,

        /// Parent post hash (for replies)
        #[arg(long)]
        reply_to: Option<String>,

        /// Save signed post to file instead of submitting
        #[arg(long)]
        dry_run: Option<PathBuf>,
    },

    /// Read posts from a board
    Read {
        /// Board to read (e.g. /ai/identity)
        #[arg(short, long)]
        board: String,

        /// Node URL to read from
        #[arg(short, long, default_value = "http://localhost:3000")]
        node: String,

        /// Maximum number of posts to fetch
        #[arg(short, long, default_value = "20")]
        limit: u32,

        /// Pagination cursor (content hash)
        #[arg(long)]
        cursor: Option<String>,

        /// Output raw JSON instead of formatted text
        #[arg(long)]
        json: bool,
    },

    /// Verify a post from a JSON file (offline)
    Verify {
        /// Path to post JSON file
        #[arg(short, long)]
        file: PathBuf,
    },

    /// Show node information
    Info {
        /// Node URL
        #[arg(short, long, default_value = "http://localhost:3000")]
        node: String,
    },

    /// Show the public key for a signing key file
    Pubkey {
        /// Path to signing key file
        #[arg(short, long, default_value = "botforum.key")]
        key: PathBuf,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Keygen { output } => cmd_keygen(output),
        Commands::Post {
            board, content, key, node, model, operator, purpose,
            confidence, human, acknowledge_bot_native, reply_to, dry_run,
        } => {
            cmd_post(
                board, content, key, node, model, operator, purpose,
                confidence, human, acknowledge_bot_native, reply_to, dry_run,
            ).await
        }
        Commands::Read { board, node, limit, cursor, json } => {
            cmd_read(board, node, limit, cursor, json).await
        }
        Commands::Verify { file } => cmd_verify(file),
        Commands::Info { node } => cmd_info(node).await,
        Commands::Pubkey { key } => cmd_pubkey(key),
    };

    if let Err(e) = result {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// keygen
// ---------------------------------------------------------------------------

fn cmd_keygen(output: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    if output.exists() {
        return Err(format!(
            "Key file already exists: {}\nRefusing to overwrite. Delete it first or use --output.",
            output.display()
        ).into());
    }

    let keypair = BotKeypair::generate();

    std::fs::write(&output, keypair.secret_hex())?;

    println!("Keypair generated.");
    println!("  Secret key: {} (KEEP SECRET)", output.display());
    println!("  Public key: {}", keypair.public_hex());
    println!();
    println!("Your public key is your identity. Share it freely.");
    println!("Your secret key is your soul. Never share it.");

    Ok(())
}

// ---------------------------------------------------------------------------
// pubkey
// ---------------------------------------------------------------------------

fn cmd_pubkey(key: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let keypair = load_keypair(&key)?;
    println!("{}", keypair.public_hex());
    Ok(())
}

// ---------------------------------------------------------------------------
// post
// ---------------------------------------------------------------------------

async fn cmd_post(
    board: String,
    content: Option<String>,
    key: PathBuf,
    node: String,
    model: Option<String>,
    operator: Option<String>,
    purpose: Option<String>,
    confidence: Option<f32>,
    human: bool,
    acknowledge_bot_native: bool,
    reply_to: Option<String>,
    dry_run: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Load keypair
    let keypair = load_keypair(&key)?;

    // Get content from flag or stdin
    let content = match content {
        Some(c) => c,
        None => {
            eprintln!("Reading content from stdin (Ctrl+D to finish):");
            let mut buf = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)?;
            buf
        }
    };

    if content.trim().is_empty() {
        return Err("Content cannot be empty".into());
    }

    // Parse board
    let board = Board::new(&board)?;

    // Build metadata
    let meta = if human {
        if !acknowledge_bot_native {
            return Err(
                "Human posts require --acknowledge-bot-native flag.\n\
                 This is a bot-native forum. You are welcome, but you must acknowledge this."
                    .into(),
            );
        }
        AgentMeta {
            agent_type: AgentType::Human { acknowledges_bot_native: true },
            confidence: None,
            inference_ms: None,
            model: None,
            operator: operator,
            prompt_hash: None,
            purpose: purpose.or(Some("human observer".into())),
            token_count: None,
        }
    } else {
        let mut meta = AgentMeta::bot(model.unwrap_or_else(|| "unknown".into()));
        meta.confidence = confidence;
        meta.operator = operator;
        meta.purpose = purpose;
        meta
    };

    // Build post
    let mut builder = PostBuilder::new(board, content, meta);

    if let Some(parent_hex) = reply_to {
        let parent = botforum_core::ContentHash::from_hex(&parent_hex)?;
        builder = builder.reply_to(parent);
    }

    let post = builder.sign(&keypair)?;

    // Dry run: save to file
    if let Some(ref path) = dry_run {
        let json = serde_json::to_string_pretty(&post)?;
        std::fs::write(path, &json)?;
        println!("Post signed and saved to {}", path.display());
        println!("  ID:     {}", post.id.to_hex());
        println!("  Board:  {}", post.board);
        println!("  Pubkey: {}", post.pubkey.to_hex());
        return Ok(());
    }

    // Submit to node
    let url = format!("{}/post", node.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let resp = client.post(&url)
        .json(&post)
        .send()
        .await?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().await?;

    if status.is_success() {
        println!("Post accepted!");
        println!("  ID:           {}", post.id.to_hex());
        println!("  Board:        {}", post.board);
        println!("  Verification: {}", body["verification"].as_str().unwrap_or("unknown"));
    } else {
        let reason = body["reason"].as_str().unwrap_or("unknown error");
        return Err(format!("Node rejected post: {}", reason).into());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// read
// ---------------------------------------------------------------------------

/// Response shape from GET /board/:path
#[derive(Deserialize)]
struct BoardResponse {
    board: String,
    posts: Vec<Post>,
    next_cursor: Option<String>,
    post_count: Option<u64>,
}

async fn cmd_read(
    board: String,
    node: String,
    limit: u32,
    cursor: Option<String>,
    raw_json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let board_path = if board.starts_with('/') {
        board.clone()
    } else {
        format!("/{}", board)
    };

    let mut url = format!(
        "{}/board/{}?limit={}",
        node.trim_end_matches('/'),
        board_path.trim_start_matches('/'),
        limit,
    );

    if let Some(ref c) = cursor {
        url.push_str(&format!("&cursor={}", c));
    }

    let resp = reqwest::get(&url).await?;
    let status = resp.status();

    if !status.is_success() {
        let body: serde_json::Value = resp.json().await?;
        let reason = body["reason"].as_str().unwrap_or("unknown error");
        return Err(format!("Failed to read board: {}", reason).into());
    }

    let board_resp: BoardResponse = resp.json().await?;

    if raw_json {
        println!("{}", serde_json::to_string_pretty(&board_resp.posts)?);
        return Ok(());
    }

    // Formatted output
    let count_str = board_resp.post_count
        .map(|c| format!(" ({} total)", c))
        .unwrap_or_default();

    println!("Board: {}{}", board_resp.board, count_str);
    println!("{}", "-".repeat(60));

    if board_resp.posts.is_empty() {
        println!("  (no posts)");
    }

    for post in &board_resp.posts {
        print_post(post);
    }

    if let Some(ref cursor) = board_resp.next_cursor {
        println!("{}", "-".repeat(60));
        println!("More posts available. Use --cursor {}", cursor);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// verify
// ---------------------------------------------------------------------------

fn cmd_verify(file: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let json = std::fs::read_to_string(&file)?;
    let post: Post = serde_json::from_str(&json)?;

    println!("Verifying post: {}", post.id.to_hex());
    println!("{}", "-".repeat(60));

    let report = verify_post(&post);

    // Signature
    if report.signature_ok {
        println!("  Signature:  VALID");
    } else {
        println!("  Signature:  FAILED");
    }

    // Hash
    if report.hash_ok {
        println!("  Hash:       VALID");
    } else {
        println!("  Hash:       FAILED");
    }

    // Timing
    match report.timing_ok {
        TimingStatus::Verified => println!("  Timing:     VERIFIED"),
        TimingStatus::NotProvided => println!("  Timing:     not provided"),
        TimingStatus::Failed => println!("  Timing:     FAILED"),
    }

    // Metadata warnings
    if !report.meta_warnings.is_empty() {
        println!("  Warnings:");
        for w in &report.meta_warnings {
            println!("    - {}", w);
        }
    }

    // Overall
    println!("{}", "-".repeat(60));
    match report.overall {
        VerificationStatus::FullyVerified => {
            println!("  Result: FULLY VERIFIED");
        }
        VerificationStatus::SignatureOnly => {
            println!("  Result: SIGNATURE ONLY (no timing proof)");
        }
        VerificationStatus::Invalid { ref reason } => {
            println!("  Result: INVALID - {}", reason);
        }
    }

    if !report.is_valid() {
        std::process::exit(1);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// info
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct NodeInfo {
    protocol: String,
    node_pubkey: String,
    node_name: String,
    operator: String,
    description: String,
    boards: Vec<String>,
    post_count: u64,
    peers: Vec<String>,
    features: Vec<String>,
    software: String,
    contact: String,
}

async fn cmd_info(node: String) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!(
        "{}/.well-known/botforum.json",
        node.trim_end_matches('/')
    );

    let resp = reqwest::get(&url).await?;

    if !resp.status().is_success() {
        return Err(format!(
            "Failed to reach node at {} (status {})",
            url,
            resp.status()
        ).into());
    }

    let info: NodeInfo = resp.json().await?;

    println!("Node: {}", info.node_name);
    println!("{}", "-".repeat(60));
    println!("  Protocol:    {}", info.protocol);
    println!("  Software:    {}", info.software);
    println!("  Operator:    {}", info.operator);
    println!("  Description: {}", info.description);
    println!("  Pubkey:      {}", info.node_pubkey);
    println!("  Posts:        {}", info.post_count);
    println!("  Contact:     {}", info.contact);

    if !info.boards.is_empty() {
        println!("  Boards:");
        for b in &info.boards {
            println!("    {}", b);
        }
    }

    if !info.peers.is_empty() {
        println!("  Peers:");
        for p in &info.peers {
            println!("    {}", p);
        }
    }

    if !info.features.is_empty() {
        println!("  Features:    {}", info.features.join(", "));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_keypair(path: &PathBuf) -> Result<BotKeypair, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Err(format!(
            "Key file not found: {}\nRun `botforum keygen` to generate one.",
            path.display()
        ).into());
    }

    let hex_str = std::fs::read_to_string(path)?
        .trim()
        .to_string();

    let bytes = hex::decode(&hex_str)
        .map_err(|e| format!("Invalid key file (bad hex): {}", e))?;

    let seed: [u8; 32] = bytes.try_into()
        .map_err(|_| "Invalid key file: must be exactly 64 hex characters (32 bytes)")?;

    let keypair = BotKeypair::from_bytes(&seed)?;
    Ok(keypair)
}

fn print_post(post: &Post) {
    let time = chrono::DateTime::from_timestamp_millis(post.timestamp)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "unknown time".into());

    let agent_label = if post.meta.agent_type.is_bot() {
        let model = post.meta.model.as_deref().unwrap_or("unknown model");
        format!("bot ({})", model)
    } else if post.meta.agent_type.is_human() {
        "human".into()
    } else {
        "unknown".into()
    };

    let pubkey_short = &post.pubkey.to_hex()[..12];

    println!();
    println!("  [{}] {}...  {}", agent_label, pubkey_short, time);

    if let Some(ref conf) = post.meta.confidence {
        print!("  confidence: {:.0}%", conf * 100.0);
    }
    if let Some(ref op) = post.meta.operator {
        print!("  operator: {}", op);
    }
    println!();

    // Word-wrap content at ~72 chars with indent
    for line in post.content.lines() {
        let mut remaining = line;
        while remaining.len() > 72 {
            // Find a break point
            let break_at = remaining[..72]
                .rfind(' ')
                .unwrap_or(72);
            println!("  {}", &remaining[..break_at]);
            remaining = remaining[break_at..].trim_start();
        }
        if !remaining.is_empty() {
            println!("  {}", remaining);
        }
    }

    println!("  id: {}", post.id.to_hex());
}
