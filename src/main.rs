use claude_code_proxy::{config::ProxyConfig, proxy::ClaudeToGeminiProxy};
use pingora::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // Load configuration
    let config = ProxyConfig::from_env()?;
    config.validate()?;

    eprintln!("Starting Claude-to-Gemini proxy...");
    eprintln!("  Listen: {}", config.server.listen_addr);
    eprintln!("  Workers: {}", config.server.workers);
    eprintln!("  Gemini endpoint: {}", config.gemini.endpoint);

    // Create Pingora server
    let mut server = Server::new(None)?;
    server.bootstrap();

    // Create proxy
    let proxy = ClaudeToGeminiProxy::new(config.clone());

    // Create HTTP proxy service
    let mut service = http_proxy_service(&server.configuration, proxy);
    service.add_tcp(&config.server.listen_addr);

    eprintln!("Proxy ready!");

    // Start server
    server.add_service(service);
    server.run_forever();
}
