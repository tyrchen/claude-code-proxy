use axum::{
    Json,
    body::Body,
    extract::State,
    http::{Response, StatusCode},
    response::IntoResponse,
};
use bytes::{BufMut, Bytes, BytesMut};
use futures::{Stream, StreamExt};
use std::fs::OpenOptions;
use std::io::Write;
use std::pin::Pin;
use std::sync::Arc;
use tracing::{error, info};

use crate::config::{ProviderConfig, ProxyConfig};
use crate::models::claude::ClaudeRequest;
use crate::provider::Provider;
use crate::state::GLOBAL_STATE;
use crate::streaming::{SSEEventGenerator, StreamingJsonParser};
use crate::transform::{map_model_name, transform_request_with_state, validate_claude_request};
use crate::validation::validate_tools;

pub struct AppState {
    pub provider: Arc<dyn Provider>,
    pub config: ProxyConfig,
}

pub async fn handle_messages(
    State(state): State<Arc<AppState>>,
    Json(claude_req): Json<ClaudeRequest>,
) -> impl IntoResponse {
    // Validate request
    if let Err(e) = validate_claude_request(&claude_req) {
        error!("Validation failed: {}", e);
        return (StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }

    // Validate tools if present
    if let Some(ref tools) = claude_req.tools
        && let Err(e) = validate_tools(tools)
    {
        error!("Tool validation failed: {}", e);
        return (StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }

    let body: Bytes;
    let target_model: String;
    let needs_transformation = state.provider.needs_transformation();

    if needs_transformation {
        // For Gemini: Transform request
        let mapped_model = map_model_name(&claude_req.model);
        info!(
            "{}: Request for model: {} -> {}",
            state.provider.name(),
            claude_req.model,
            mapped_model
        );
        target_model = mapped_model.to_string();

        // Get auto_todo_prompt flag from config
        let auto_todo_prompt = match &state.config.provider {
            ProviderConfig::Gemini(cfg) => cfg.auto_todo_prompt,
            _ => false,
        };

        // Transform to Gemini format with state tracking
        let gemini_req = match transform_request_with_state(
            claude_req,
            Some(&*GLOBAL_STATE),
            auto_todo_prompt,
        ) {
            Ok(req) => req,
            Err(e) => {
                error!("Transformation failed: {}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
        };

        // Serialize transformed request
        body = match serde_json::to_vec(&gemini_req) {
            Ok(b) => Bytes::from(b),
            Err(e) => {
                error!("Serialization failed: {}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
        };

        info!("Transformed to {} bytes", body.len());
    } else {
        // For Kimi: Pure forwarding, no transformation needed
        info!(
            "{}: Pure forwarding for model: {}",
            state.provider.name(),
            claude_req.model
        );
        target_model = claude_req.model.clone();

        // Serialize original Claude request
        body = match serde_json::to_vec(&claude_req) {
            Ok(b) => Bytes::from(b),
            Err(e) => {
                error!("Serialization failed: {}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
        };
    }

    // Make request to provider
    let stream = match state
        .provider
        .stream_generate_content(&target_model, body)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            error!("{} request failed: {}", state.provider.name(), e);
            return (StatusCode::BAD_GATEWAY, e.to_string()).into_response();
        }
    };

    // For providers needing transformation, convert streaming JSON to SSE
    // For Kimi (pure forwarding), just pass through the stream
    if needs_transformation {
        let sse_stream = transform_to_sse(stream, target_model);
        Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/event-stream")
            .header("Cache-Control", "no-cache")
            .header("Connection", "keep-alive")
            .body(Body::from_stream(sse_stream))
            .unwrap()
    } else {
        // Pure forwarding: pass through the stream as-is with logging
        let passthrough_stream = stream.map(|chunk_result| {
            match chunk_result {
                Ok(chunk) => {
                    // Log Kimi streaming response chunks
                    if let Ok(mut file) = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open("/tmp/kimi.log")
                    {
                        let chunk_str = String::from_utf8_lossy(&chunk);
                        let _ = writeln!(file, "--- STREAMING RESPONSE CHUNK ---");
                        let _ = writeln!(file, "{}", chunk_str);
                        let _ = writeln!(file);
                    }
                    Ok(chunk)
                }
                Err(e) => Err(std::io::Error::other(e)),
            }
        });

        Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/event-stream")
            .header("Cache-Control", "no-cache")
            .header("Connection", "keep-alive")
            .body(Body::from_stream(passthrough_stream))
            .unwrap()
    }
}

fn transform_to_sse(
    stream: Pin<Box<dyn Stream<Item = reqwest::Result<Bytes>> + Send>>,
    model: String,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> {
    let mut parser = StreamingJsonParser::new();
    let mut generator = SSEEventGenerator::with_state(model, GLOBAL_STATE.clone());
    let mut outgoing_events = BytesMut::new();

    stream.map(move |chunk_result| {
        match chunk_result {
            Ok(chunk) => {
                // Log raw Gemini response
                if let Ok(mut file) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/tmp/gemini.log")
                {
                    let _ = writeln!(file, "\n=== RESPONSE CHUNK ===");
                    let _ = writeln!(file, "{}", String::from_utf8_lossy(&chunk));
                }

                // Parse Gemini JSON chunks
                let parsed_chunks = parser.feed(&chunk).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
                })?;

                // Generate SSE events
                for gemini_chunk in parsed_chunks {
                    let events = generator.generate_events(gemini_chunk);
                    for event in events {
                        if event.contains("message_start") || event.contains("message_stop") {
                            info!("SSE: {}", event.lines().next().unwrap_or(""));
                        }
                        // Log SSE output
                        if let Ok(mut file) = OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open("/tmp/gemini.log")
                        {
                            let _ = writeln!(file, "\n=== SSE EVENT ===");
                            let _ = writeln!(file, "{}", event);
                        }
                        outgoing_events.put(event.as_bytes());
                    }
                }

                // Always return what we have (even if empty)
                if !outgoing_events.is_empty() {
                    Ok(outgoing_events.split().freeze())
                } else {
                    Ok(Bytes::new())
                }
            }
            Err(e) => Err(std::io::Error::other(e.to_string())),
        }
    })
}
