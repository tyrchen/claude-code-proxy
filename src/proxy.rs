use async_trait::async_trait;
use bytes::{BufMut, Bytes, BytesMut};
use pingora::http::ResponseHeader;
use pingora::prelude::*;
use std::sync::Arc;
use std::time::Duration;

use crate::config::ProxyConfig;
use crate::models::claude::ClaudeRequest;
use crate::streaming::{SSEEventGenerator, StreamingJsonParser};
use crate::transform::{map_model_name, transform_request, validate_claude_request};

/// Per-request context for the proxy
pub struct RequestContext {
    // Request transformation state
    pub transformed_body: Option<Bytes>,
    pub target_model: String,

    // Response streaming state
    pub parser: StreamingJsonParser,
    pub event_generator: SSEEventGenerator,

    // Output buffering
    pub outgoing_events: BytesMut,

    // Error state
    pub upstream_error: bool,
}

impl RequestContext {
    pub fn new() -> Self {
        Self {
            transformed_body: None,
            target_model: "gemini-2.0-flash-exp".to_string(),
            parser: StreamingJsonParser::new(),
            event_generator: SSEEventGenerator::new("gemini-2.0-flash-exp".to_string()),
            outgoing_events: BytesMut::new(),
            upstream_error: false,
        }
    }
}

impl Default for RequestContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Main proxy implementation
pub struct ClaudeToGeminiProxy {
    config: Arc<ProxyConfig>,
}

impl ClaudeToGeminiProxy {
    pub fn new(config: ProxyConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }
}

#[async_trait]
impl ProxyHttp for ClaudeToGeminiProxy {
    type CTX = RequestContext;

    fn new_ctx(&self) -> Self::CTX {
        RequestContext::new()
    }

    async fn upstream_peer(
        &self,
        _session: &mut Session,
        _ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        let peer = Box::new(HttpPeer::new(
            (&self.config.gemini.endpoint as &str, 443),
            true,                                // TLS
            self.config.gemini.endpoint.clone(), // SNI
        ));

        Ok(peer)
    }

    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool> {
        // Read full request body
        let body_bytes = match session.read_request_body().await? {
            Some(b) => b,
            None => {
                return Err(Error::explain(
                    ErrorType::InvalidHTTPHeader,
                    "Empty request body",
                ));
            }
        };

        // Parse Claude request
        let claude_req: ClaudeRequest = serde_json::from_slice(&body_bytes).map_err(|e| {
            Error::explain(
                ErrorType::InvalidHTTPHeader,
                format!("Invalid Claude request: {}", e),
            )
        })?;

        // Validate
        validate_claude_request(&claude_req).map_err(|e| {
            Error::explain(
                ErrorType::InvalidHTTPHeader,
                format!("Validation failed: {}", e),
            )
        })?;

        // Map model
        ctx.target_model = map_model_name(&claude_req.model).to_string();
        ctx.event_generator = SSEEventGenerator::new(ctx.target_model.clone());

        // Transform to Gemini format
        let gemini_req = transform_request(claude_req).map_err(|e| {
            Error::explain(
                ErrorType::InternalError,
                format!("Transformation failed: {}", e),
            )
        })?;

        // Serialize
        let transformed = serde_json::to_vec(&gemini_req).map_err(|e| {
            Error::explain(
                ErrorType::InternalError,
                format!("Serialization failed: {}", e),
            )
        })?;

        ctx.transformed_body = Some(Bytes::from(transformed));

        Ok(false) // Continue processing
    }

    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        let header = upstream_request;

        // Rewrite URL for Gemini API
        let new_uri = format!(
            "/v1beta/models/{}:streamGenerateContent?key={}",
            ctx.target_model, self.config.gemini.api_key
        );
        header.set_uri(
            new_uri
                .parse()
                .map_err(|e| Error::explain(ErrorType::InvalidHTTPHeader, format!("{}", e)))?,
        );

        // Set headers
        header.insert_header("Host", &self.config.gemini.endpoint)?;
        header.insert_header("Content-Type", "application/json")?;
        header.remove_header("x-api-key");

        if let Some(body) = &ctx.transformed_body {
            header.insert_header("Content-Length", body.len().to_string())?;
        }

        Ok(())
    }

    async fn request_body_filter(
        &self,
        _session: &mut Session,
        body: &mut Option<Bytes>,
        _end: bool,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        // Replace body with transformed version
        *body = ctx.transformed_body.take();
        Ok(())
    }

    async fn response_filter(
        &self,
        _session: &mut Session,
        upstream_response: &mut ResponseHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        // Check for upstream errors
        if upstream_response.status != 200 {
            ctx.upstream_error = true;
            // Will handle error in response_body_filter
            return Ok(());
        }

        // Set SSE headers for successful responses
        upstream_response.insert_header("Content-Type", "text/event-stream")?;
        upstream_response.insert_header("Cache-Control", "no-cache")?;
        upstream_response.insert_header("Connection", "keep-alive")?;
        upstream_response.remove_header("Content-Length"); // Chunked encoding

        Ok(())
    }

    fn response_body_filter(
        &self,
        _session: &mut Session,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> Result<Option<Duration>> {
        // Handle upstream errors
        if ctx.upstream_error {
            if let Some(error_body) = body.take() {
                // Try to parse Gemini error
                let error_msg = String::from_utf8_lossy(&error_body);
                let error_sse = SSEEventGenerator::format_error("api_error", &error_msg);
                *body = Some(Bytes::from(error_sse));
            }
            return Ok(None);
        }

        // Feed new data to parser
        if let Some(chunk) = body.take() {
            let parsed_chunks = ctx.parser.feed(&chunk).map_err(|e| {
                Error::explain(ErrorType::InternalError, format!("Parse error: {}", e))
            })?;

            // Generate SSE events
            for gemini_chunk in parsed_chunks {
                let events = ctx.event_generator.generate_events(gemini_chunk);
                for event in events {
                    ctx.outgoing_events.put(event.as_bytes());
                }
            }
        }

        // Output accumulated events
        if !ctx.outgoing_events.is_empty() {
            *body = Some(ctx.outgoing_events.split().freeze());
        }

        // Handle stream end
        if end_of_stream && body.is_none() {
            if !ctx.event_generator.is_header_sent() {
                eprintln!("Warning: Stream ended without data");
            }
        }

        Ok(None)
    }

    async fn logging(
        &self,
        session: &mut Session,
        _e: Option<&pingora::Error>,
        ctx: &mut Self::CTX,
    ) {
        let uri = session.req_header().uri.path();
        let status = session
            .response_written()
            .map(|r| r.status.as_u16())
            .unwrap_or(0);

        let (input_tokens, output_tokens) = ctx.event_generator.token_counts();

        eprintln!(
            "Request: {} -> {} (model: {}, tokens: {}in/{}out)",
            uri, status, ctx.target_model, input_tokens, output_tokens
        );
    }
}
