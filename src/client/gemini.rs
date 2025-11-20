use bytes::Bytes;
use reqwest::Client;
use std::fs::OpenOptions;
use std::io::Write;
use tracing::info;

use crate::config::GeminiConfig;
use crate::error::{ProxyError, Result};
use crate::provider::{Provider, ProviderStream, StreamFuture};

pub struct GeminiClient {
    client: Client,
    config: GeminiConfig,
}

impl GeminiClient {
    pub fn new(config: GeminiConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| {
                ProxyError::InternalError(format!("Failed to create HTTP client: {}", e))
            })?;

        Ok(Self { client, config })
    }
}

impl Provider for GeminiClient {
    fn stream_generate_content(&self, model: &str, body: Bytes) -> StreamFuture {
        let url = format!(
            "https://{}/v1beta/models/{}:streamGenerateContent?key={}",
            self.config.endpoint, model, self.config.api_key
        );
        let body = body.clone();
        let client = self.client.clone();
        let api_key = self.config.api_key.clone();

        Box::pin(
            async move { Self::stream_generate_content_impl(url, body, client, api_key).await },
        )
    }

    fn needs_transformation(&self) -> bool {
        true // Gemini needs Claude->Gemini transformation
    }

    fn name(&self) -> &str {
        "Gemini"
    }
}

impl GeminiClient {
    async fn stream_generate_content_impl(
        url: String,
        body: Bytes,
        client: Client,
        api_key: String,
    ) -> Result<ProviderStream> {
        info!(
            "Gemini: Sending {} bytes to: {}",
            body.len(),
            url.split("?key=").next().unwrap_or(&url)
        );

        // Log request body
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/gemini.log")
        {
            let _ = writeln!(file, "\n=== REQUEST ===");
            let _ = writeln!(file, "{}", String::from_utf8_lossy(&body));
        }

        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Content-Length", body.len())
            .header("x-goog-api-key", &api_key)
            .body(body)
            .send()
            .await
            .map_err(|e| ProxyError::UpstreamError(format!("Gemini request failed: {}", e)))?;

        let status = response.status();
        info!("Gemini responded with status: {}", status);

        if !status.is_success() {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ProxyError::UpstreamError(format!(
                "Gemini API error {}: {}",
                status, error_body
            )));
        }

        Ok(Box::pin(response.bytes_stream()))
    }
}
