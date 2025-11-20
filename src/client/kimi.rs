use bytes::Bytes;
use reqwest::Client;
use std::fs::OpenOptions;
use std::io::Write;
use tracing::info;

use crate::config::KimiConfig;
use crate::error::{ProxyError, Result};
use crate::provider::{Provider, ProviderStream, StreamFuture};

pub struct KimiClient {
    client: Client,
    config: KimiConfig,
}

impl KimiClient {
    pub fn new(config: KimiConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| {
                ProxyError::InternalError(format!("Failed to create HTTP client: {}", e))
            })?;

        Ok(Self { client, config })
    }
}

impl Provider for KimiClient {
    fn stream_generate_content(&self, _model: &str, body: Bytes) -> StreamFuture {
        let url = format!("{}/v1/messages", self.config.endpoint);
        let body = body.clone();
        let client = self.client.clone();
        let api_key = self.config.api_key.clone();
        let model = self.config.model.clone();

        Box::pin(async move {
            Self::stream_generate_content_impl(url, body, client, api_key, model).await
        })
    }

    fn needs_transformation(&self) -> bool {
        false // Kimi is Claude-compatible, no transformation needed
    }

    fn name(&self) -> &str {
        "Kimi"
    }
}

impl KimiClient {
    async fn stream_generate_content_impl(
        url: String,
        body: Bytes,
        client: Client,
        api_key: String,
        model: String,
    ) -> Result<ProviderStream> {
        info!(
            "Kimi: Sending {} bytes to: {} with model: {}",
            body.len(),
            url,
            model
        );

        // Log request body to debug file
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/kimi.log")
        {
            let _ = writeln!(file, "\n{}", "=".repeat(80));
            let _ = writeln!(file, "=== KIMI REQUEST ({} bytes) ===", body.len());
            let _ = writeln!(file, "{}", "=".repeat(80));
            let _ = writeln!(file, "URL: {}", url);
            let _ = writeln!(file, "Model: {}", model);
            let _ = writeln!(file, "\n--- REQUEST BODY (Pretty JSON) ---");
            if let Ok(json_value) = serde_json::from_slice::<serde_json::Value>(&body) {
                let _ = writeln!(
                    file,
                    "{}",
                    serde_json::to_string_pretty(&json_value).unwrap_or_default()
                );
            } else {
                let _ = writeln!(file, "{}", String::from_utf8_lossy(&body));
            }
            let _ = writeln!(file);
        }

        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01")
            .header("x-api-key", &api_key)
            .body(body)
            .send()
            .await
            .map_err(|e| ProxyError::UpstreamError(format!("Kimi request failed: {}", e)))?;

        let status = response.status();
        info!("Kimi responded with status: {}", status);

        // Log response status
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/kimi.log")
        {
            let _ = writeln!(file, "--- RESPONSE STATUS ---");
            let _ = writeln!(file, "Status: {}", status);
            let _ = writeln!(file);
        }

        if !status.is_success() {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            // Log error
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/kimi.log")
            {
                let _ = writeln!(file, "--- ERROR RESPONSE ---");
                let _ = writeln!(file, "{}", error_body);
                let _ = writeln!(file, "\n");
            }

            return Err(ProxyError::UpstreamError(format!(
                "Kimi API error {}: {}",
                status, error_body
            )));
        }

        Ok(Box::pin(response.bytes_stream()))
    }
}
