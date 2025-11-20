use axum::{Router, routing::post};
use clap::{Parser, Subcommand};
use claude_code_proxy::{
    client::{GeminiClient, KimiClient},
    config::{ProviderConfig, ProxyConfig},
    handler::{AppState, handle_messages},
    provider::Provider,
};
use std::sync::Arc;
use tracing::info;

#[derive(Parser)]
#[command(name = "claude-code-proxy")]
#[command(about = "Proxy Claude Code requests to various AI providers", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Use Gemini as the backend provider (with request transformation)
    Gemini,
    /// Use Kimi as the backend provider (pure forwarding, Claude-compatible)
    Kimi,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    // Determine provider type from subcommand
    let provider_type = match cli.command {
        Commands::Gemini => "gemini",
        Commands::Kimi => "kimi",
    };

    // Load configuration
    let config = ProxyConfig::from_env(provider_type)?;
    config.validate()?;

    // Create appropriate provider client
    let provider: Arc<dyn Provider> = match &config.provider {
        ProviderConfig::Gemini(gemini_config) => {
            info!("Starting Claude-to-Gemini proxy...");
            info!("  Listen: {}", config.server.listen_addr);
            info!("  Gemini endpoint: {}", gemini_config.endpoint);
            Arc::new(GeminiClient::new(gemini_config.clone())?)
        }
        ProviderConfig::Kimi(kimi_config) => {
            info!("Starting Claude-to-Kimi proxy...");
            info!("  Listen: {}", config.server.listen_addr);
            info!("  Kimi endpoint: {}", kimi_config.endpoint);
            info!("  Kimi model: {}", kimi_config.model);
            Arc::new(KimiClient::new(kimi_config.clone())?)
        }
    };

    // Clear any stale state from previous runs
    claude_code_proxy::state::GLOBAL_STATE.clear();
    claude_code_proxy::cache::TOOL_CACHE.clear();
    claude_code_proxy::metrics::TOOL_METRICS.reset();
    info!("  State cleared");

    // Create app state
    let state = Arc::new(AppState {
        provider,
        config: config.clone(),
    });

    // Build router
    let app = Router::new()
        .route("/v1/messages", post(handle_messages))
        .with_state(state);

    info!("Proxy ready!");

    // Start server
    let listener = tokio::net::TcpListener::bind(&config.server.listen_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
