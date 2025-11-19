# Claude-to-Gemini Protocol Proxy: Detailed Design Specification

## Document Information

- **Project**: claude-code-proxy
- **Version**: 1.0
- **Date**: 2025-11-18
- **Status**: Draft

## 1. Executive Summary

### 1.1 Problem Statement

The Claude Code CLI tool is tightly coupled to Anthropic's proprietary API. However, Google's Gemini models (particularly 2.0 Flash and 1.5 Pro) offer competitive advantages in cost, context window size, and inference speed. There is no native way to use Gemini models with Claude Code.

### 1.2 Solution Overview

Build a high-performance, protocol-translating reverse proxy using Rust and Cloudflare's Pingora framework that:

1. Intercepts requests from Claude Code intended for Anthropic's API
2. Transforms the request payload from Claude's Messages API format to Gemini's GenerateContent format
3. Forwards the transformed request to Google's Gemini API
4. Streams back responses, converting Gemini's chunked JSON array format to Claude's Server-Sent Events (SSE) format
5. Maintains transparency to the client - Claude Code believes it's talking to Claude

### 1.3 Key Design Goals

- **Zero Latency Overhead**: Minimize proxy-induced latency through zero-copy buffer management
- **Protocol Fidelity**: Ensure 100% compatibility with Claude Code's expectations
- **Streaming Performance**: Handle real-time streaming with proper backpressure
- **Production Ready**: Include error handling, logging, and security best practices
- **Extensibility**: Design for future multi-model support (OpenAI, DeepSeek, etc.)

---

## 2. Architecture Overview

### 2.1 System Context

```
┌─────────────────┐
│   Claude Code   │
│   CLI Client    │
└────────┬────────┘
         │ HTTP/1.1
         │ POST /v1/messages
         │ Content-Type: application/json
         │ x-api-key: <user-provided>
         │
         ▼
┌─────────────────────────────────────────┐
│      Pingora Proxy (This System)       │
│  ┌───────────────────────────────────┐  │
│  │  Request Pipeline                 │  │
│  │  1. Parse Claude request          │  │
│  │  2. Map model names               │  │
│  │  3. Transform message structure   │  │
│  │  4. Convert system instructions   │  │
│  └───────────────────────────────────┘  │
│  ┌───────────────────────────────────┐  │
│  │  Response Pipeline                │  │
│  │  1. Buffer incomplete JSON chunks │  │
│  │  2. Parse Gemini response objects │  │
│  │  3. Generate SSE events           │  │
│  │  4. Stream to client              │  │
│  └───────────────────────────────────┘  │
└────────┬────────────────────────────────┘
         │ HTTP/2
         │ POST /v1beta/models/{model}:streamGenerateContent
         │ Content-Type: application/json
         │ x-goog-api-key: <transformed>
         │
         ▼
┌─────────────────┐
│  Google Gemini  │
│      API        │
└─────────────────┘
```

### 2.2 Core Components

#### 2.2.1 Protocol Adapters

**ClaudeRequestAdapter**
- Deserializes incoming Claude Messages API requests
- Validates request structure
- Extracts key fields: model, messages, system, max_tokens, stream

**GeminiRequestBuilder**
- Constructs Gemini GenerateContent requests
- Maps Claude concepts to Gemini equivalents
- Handles systemInstruction wrapping
- Builds generationConfig

**GeminiResponseParser**
- Implements stateful JSON streaming parser
- Handles chunked transfer encoding
- Detects complete JSON objects using brace counting
- Emits parsed response chunks

**SSEEventGenerator**
- Converts Gemini response chunks to Claude SSE events
- Maintains SSE event sequence (message_start → content_block_delta → message_stop)
- Handles timing and ordering constraints

#### 2.2.2 Pingora Integration

**ClaudeToGeminiProxy** (implements `ProxyHttp` trait)
- `new_ctx()`: Initializes per-request context
- `upstream_peer()`: Selects Google API endpoint with TLS/SNI configuration
- `request_filter()`: Transforms request headers and body
- `upstream_request_filter()`: Injects transformed body
- `response_filter()`: Sets SSE response headers
- `response_body_filter()`: Implements streaming transformation state machine

**ProxyContext** (per-request state)
- `response_buffer: BytesMut`: Accumulates incomplete JSON chunks
- `json_depth: i32`: Tracks brace nesting depth for parsing
- `header_sent: bool`: Ensures message_start is sent exactly once
- `target_model: String`: Resolved Gemini model name
- `content_block_index: u32`: Tracks SSE content block numbering
- `in_string: bool`, `escaped: bool`: JSON string parsing state

---

## 3. Data Model Specification

### 3.1 Request Mapping

#### 3.1.1 Claude Messages API Request

```rust
#[derive(Debug, Deserialize)]
struct ClaudeRequest {
    /// Model identifier (e.g., "claude-3-5-sonnet-20241022")
    model: String,

    /// Conversation history with strict user/assistant alternation
    messages: Vec<ClaudeMessage>,

    /// Optional system prompt (top-level field)
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<SystemPrompt>,

    /// Maximum tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,

    /// Temperature (0.0-1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,

    /// Stop sequences
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequences: Option<Vec<String>>,

    /// Enable streaming
    #[serde(default)]
    stream: bool,

    /// Top-P sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,

    /// Top-K sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ClaudeMessage {
    /// "user" or "assistant"
    role: String,

    /// Either a string or array of content blocks
    content: ContentType,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ContentType {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,  // "text", "image", "tool_use", "tool_result"

    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,

    // Additional fields for images, tools, etc.
    #[serde(flatten)]
    extra: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SystemPrompt {
    Text(String),
    Blocks(Vec<ContentBlock>),
}
```

#### 3.1.2 Gemini GenerateContent Request

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiRequest {
    /// Conversation history
    contents: Vec<GeminiContent>,

    /// System instructions (wrapped in special structure)
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,

    /// Generation parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,

    /// Safety settings
    #[serde(skip_serializing_if = "Option::is_none")]
    safety_settings: Option<Vec<SafetySetting>>,
}

#[derive(Debug, Serialize)]
struct GeminiContent {
    /// "user" or "model" (NOT "assistant")
    role: String,

    /// Always an array, even for single text
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum GeminiPart {
    Text { text: String },
    InlineData {
        inline_data: InlineData
    },
    // Future: function calling, etc.
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InlineData {
    mime_type: String,
    data: String,  // base64
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequences: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SafetySetting {
    category: String,
    threshold: String,
}
```

### 3.2 Response Mapping

#### 3.2.1 Claude SSE Event Stream

Claude emits a sequence of Server-Sent Events:

```
event: message_start
data: {"type":"message_start","message":{"id":"msg_xxx","type":"message","role":"assistant","model":"claude-3-5-sonnet-20241022","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":0}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" world"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":5}}

event: message_stop
data: {"type":"message_stop"}
```

**Event Type Specifications:**

```rust
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ClaudeSSEEvent {
    #[serde(rename = "message_start")]
    MessageStart {
        message: MessageMetadata,
    },

    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: u32,
        content_block: ContentBlockMetadata,
    },

    #[serde(rename = "content_block_delta")]
    ContentBlockDelta {
        index: u32,
        delta: Delta,
    },

    #[serde(rename = "content_block_stop")]
    ContentBlockStop {
        index: u32,
    },

    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: MessageDeltaData,
        usage: UsageInfo,
    },

    #[serde(rename = "message_stop")]
    MessageStop,

    #[serde(rename = "ping")]
    Ping,

    #[serde(rename = "error")]
    Error {
        error: ErrorInfo,
    },
}

#[derive(Debug, Serialize)]
struct MessageMetadata {
    id: String,
    #[serde(rename = "type")]
    msg_type: String,  // "message"
    role: String,  // "assistant"
    model: String,
    content: Vec<serde_json::Value>,
    stop_reason: Option<String>,
    stop_sequence: Option<String>,
    usage: UsageInfo,
}

#[derive(Debug, Serialize)]
struct ContentBlockMetadata {
    #[serde(rename = "type")]
    block_type: String,  // "text"
    text: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum Delta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
}

#[derive(Debug, Serialize)]
struct MessageDeltaData {
    stop_reason: Option<String>,
    stop_sequence: Option<String>,
}

#[derive(Debug, Serialize)]
struct UsageInfo {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Debug, Serialize)]
struct ErrorInfo {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
}
```

#### 3.2.2 Gemini Streaming Response

Gemini returns a JSON array via chunked transfer encoding:

```json
[
  {"candidates":[{"content":{"parts":[{"text":"Hello"}],"role":"model"}}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":1}},
  {"candidates":[{"content":{"parts":[{"text":" world"}],"role":"model"}}]},
  {"candidates":[{"content":{"parts":[{"text":"!"}],"role":"model"},"finishReason":"STOP"}],"usageMetadata":{"candidatesTokenCount":5,"totalTokenCount":15}}
]
```

**Response Structure:**

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiStreamChunk {
    #[serde(default)]
    candidates: Vec<Candidate>,

    #[serde(skip_serializing_if = "Option::is_none")]
    usage_metadata: Option<UsageMetadata>,

    #[serde(skip_serializing_if = "Option::is_none")]
    prompt_feedback: Option<PromptFeedback>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<GeminiContent>,

    #[serde(skip_serializing_if = "Option::is_none")]
    finish_reason: Option<String>,  // "STOP", "MAX_TOKENS", "SAFETY", etc.

    #[serde(skip_serializing_if = "Option::is_none")]
    safety_ratings: Option<Vec<SafetyRating>>,

    index: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageMetadata {
    prompt_token_count: Option<u32>,
    candidates_token_count: Option<u32>,
    total_token_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptFeedback {
    block_reason: Option<String>,
    safety_ratings: Option<Vec<SafetyRating>>,
}

#[derive(Debug, Deserialize)]
struct SafetyRating {
    category: String,
    probability: String,
}
```

---

## 4. Transformation Logic

### 4.1 Model Name Mapping

```rust
/// Maps Claude model identifiers to appropriate Gemini models
fn map_model_name(claude_model: &str) -> &'static str {
    // Fuzzy matching to handle version suffixes
    if claude_model.contains("opus") {
        "gemini-1.5-pro"  // Opus -> Pro (highest capability)
    } else if claude_model.contains("sonnet") {
        "gemini-2.0-flash-exp"  // Sonnet -> Flash (balanced)
    } else if claude_model.contains("haiku") {
        "gemini-2.0-flash-exp"  // Haiku -> Flash (speed)
    } else {
        "gemini-2.0-flash-exp"  // Default fallback
    }
}
```

**Rationale:**
- Claude Opus (most capable) → Gemini 1.5 Pro (2M context, highest quality)
- Claude Sonnet (balanced) → Gemini 2.0 Flash (fastest, free tier)
- Claude Haiku (fastest) → Gemini 2.0 Flash (speed optimized)

### 4.2 Request Transformation Algorithm

```rust
fn transform_request(claude_req: ClaudeRequest) -> Result<GeminiRequest> {
    // 1. Convert messages
    let mut contents = Vec::new();
    for msg in claude_req.messages {
        let role = match msg.role.as_str() {
            "assistant" => "model",  // CRITICAL: role name change
            "user" => "user",
            _ => return Err(Error::InvalidRole(msg.role)),
        };

        let parts = extract_parts(msg.content)?;

        contents.push(GeminiContent {
            role: role.to_string(),
            parts,
        });
    }

    // 2. Convert system prompt (if present)
    let system_instruction = claude_req.system.map(|sys| {
        let parts = match sys {
            SystemPrompt::Text(text) => vec![GeminiPart::Text { text }],
            SystemPrompt::Blocks(blocks) => blocks
                .into_iter()
                .filter_map(|b| b.text.map(|text| GeminiPart::Text { text }))
                .collect(),
        };

        GeminiSystemInstruction { parts }
    });

    // 3. Map generation config
    let generation_config = Some(GenerationConfig {
        max_output_tokens: claude_req.max_tokens,
        temperature: claude_req.temperature,
        top_p: claude_req.top_p,
        top_k: claude_req.top_k,
        stop_sequences: claude_req.stop_sequences,
    });

    Ok(GeminiRequest {
        contents,
        system_instruction,
        generation_config,
        safety_settings: None,  // Use Gemini defaults
    })
}

fn extract_parts(content: ContentType) -> Result<Vec<GeminiPart>> {
    match content {
        ContentType::Text(text) => {
            Ok(vec![GeminiPart::Text { text }])
        }
        ContentType::Blocks(blocks) => {
            let mut parts = Vec::new();
            for block in blocks {
                match block.block_type.as_str() {
                    "text" => {
                        if let Some(text) = block.text {
                            parts.push(GeminiPart::Text { text });
                        }
                    }
                    "image" => {
                        // Extract image data and convert to inline_data
                        // TODO: Implement image handling
                        log::warn!("Image blocks not yet supported");
                    }
                    _ => {
                        log::warn!("Unsupported block type: {}", block.block_type);
                    }
                }
            }
            Ok(parts)
        }
    }
}
```

### 4.3 Response Transformation Algorithm

#### 4.3.1 Streaming Parser State Machine

```rust
/// Stateful parser for Gemini's chunked JSON array stream
struct StreamingJsonParser {
    buffer: BytesMut,
    brace_depth: i32,
    in_string: bool,
    escaped: bool,
    array_started: bool,
}

impl StreamingJsonParser {
    fn new() -> Self {
        Self {
            buffer: BytesMut::with_capacity(4096),
            brace_depth: 0,
            in_string: false,
            escaped: false,
            array_started: false,
        }
    }

    /// Feed new data and extract complete JSON objects
    fn feed(&mut self, chunk: &[u8]) -> Vec<GeminiStreamChunk> {
        self.buffer.extend_from_slice(chunk);
        self.extract_objects()
    }

    fn extract_objects(&mut self) -> Vec<GeminiStreamChunk> {
        let mut results = Vec::new();

        loop {
            // Skip leading whitespace, commas, and array brackets
            self.skip_noise();

            if self.buffer.is_empty() {
                break;
            }

            // Check for array end
            if self.buffer[0] == b']' {
                self.buffer.advance(1);
                continue;
            }

            // Find complete JSON object
            if let Some(obj_end) = self.find_object_boundary() {
                let obj_bytes = self.buffer.split_to(obj_end);

                // Parse JSON object
                match serde_json::from_slice::<GeminiStreamChunk>(&obj_bytes) {
                    Ok(chunk) => results.push(chunk),
                    Err(e) => {
                        log::error!("Failed to parse Gemini chunk: {}", e);
                        log::debug!("Raw bytes: {}", String::from_utf8_lossy(&obj_bytes));
                    }
                }
            } else {
                // Incomplete object, wait for more data
                break;
            }
        }

        results
    }

    fn skip_noise(&mut self) {
        while !self.buffer.is_empty() {
            match self.buffer[0] {
                b'[' => {
                    self.array_started = true;
                    self.buffer.advance(1);
                }
                b',' | b' ' | b'\n' | b'\r' | b'\t' => {
                    self.buffer.advance(1);
                }
                _ => break,
            }
        }
    }

    fn find_object_boundary(&mut self) -> Option<usize> {
        let mut depth = 0;
        let mut in_string = false;
        let mut escaped = false;

        for (i, &byte) in self.buffer.iter().enumerate() {
            if in_string {
                if escaped {
                    escaped = false;
                } else {
                    match byte {
                        b'\\' => escaped = true,
                        b'"' => in_string = false,
                        _ => {}
                    }
                }
            } else {
                match byte {
                    b'"' => in_string = true,
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(i + 1);
                        }
                    }
                    _ => {}
                }
            }
        }

        None
    }
}
```

#### 4.3.2 SSE Event Generation

```rust
/// Converts Gemini chunks to Claude SSE events
struct SSEEventGenerator {
    header_sent: bool,
    input_tokens: u32,
    output_tokens: u32,
    model_name: String,
}

impl SSEEventGenerator {
    fn new(model_name: String) -> Self {
        Self {
            header_sent: false,
            input_tokens: 0,
            output_tokens: 0,
            model_name,
        }
    }

    fn generate_events(&mut self, chunk: GeminiStreamChunk) -> Vec<String> {
        let mut events = Vec::new();

        // Send header events on first chunk
        if !self.header_sent {
            if let Some(usage) = &chunk.usage_metadata {
                self.input_tokens = usage.prompt_token_count.unwrap_or(0);
            }

            events.push(self.format_message_start());
            events.push(self.format_content_block_start());
            self.header_sent = true;
        }

        // Process candidates
        if let Some(candidate) = chunk.candidates.first() {
            // Extract text deltas
            if let Some(content) = &candidate.content {
                for part in &content.parts {
                    if let GeminiPart::Text { text } = part {
                        if !text.is_empty() {
                            events.push(self.format_content_block_delta(text));
                            // Rough token estimation (4 chars ≈ 1 token)
                            self.output_tokens += (text.len() / 4) as u32;
                        }
                    }
                }
            }

            // Handle finish
            if let Some(finish_reason) = &candidate.finish_reason {
                let stop_reason = self.map_finish_reason(finish_reason);

                if let Some(usage) = &chunk.usage_metadata {
                    self.output_tokens = usage.candidates_token_count.unwrap_or(self.output_tokens);
                }

                events.push(self.format_content_block_stop());
                events.push(self.format_message_delta(stop_reason));
                events.push(self.format_message_stop());
            }
        }

        events
    }

    fn format_message_start(&self) -> String {
        let event = ClaudeSSEEvent::MessageStart {
            message: MessageMetadata {
                id: format!("msg_gemini_{}", uuid::Uuid::new_v4()),
                msg_type: "message".to_string(),
                role: "assistant".to_string(),
                model: self.model_name.clone(),
                content: vec![],
                stop_reason: None,
                stop_sequence: None,
                usage: UsageInfo {
                    input_tokens: self.input_tokens,
                    output_tokens: 0,
                },
            },
        };
        self.format_sse("message_start", &event)
    }

    fn format_content_block_start(&self) -> String {
        let event = ClaudeSSEEvent::ContentBlockStart {
            index: 0,
            content_block: ContentBlockMetadata {
                block_type: "text".to_string(),
                text: String::new(),
            },
        };
        self.format_sse("content_block_start", &event)
    }

    fn format_content_block_delta(&self, text: &str) -> String {
        let event = ClaudeSSEEvent::ContentBlockDelta {
            index: 0,
            delta: Delta::TextDelta {
                text: text.to_string(),
            },
        };
        self.format_sse("content_block_delta", &event)
    }

    fn format_content_block_stop(&self) -> String {
        let event = ClaudeSSEEvent::ContentBlockStop { index: 0 };
        self.format_sse("content_block_stop", &event)
    }

    fn format_message_delta(&self, stop_reason: &str) -> String {
        let event = ClaudeSSEEvent::MessageDelta {
            delta: MessageDeltaData {
                stop_reason: Some(stop_reason.to_string()),
                stop_sequence: None,
            },
            usage: UsageInfo {
                input_tokens: 0,
                output_tokens: self.output_tokens,
            },
        };
        self.format_sse("message_delta", &event)
    }

    fn format_message_stop(&self) -> String {
        let event = ClaudeSSEEvent::MessageStop;
        self.format_sse("message_stop", &event)
    }

    fn format_sse(&self, event_type: &str, data: &impl Serialize) -> String {
        let json = serde_json::to_string(data).unwrap();
        format!("event: {}\ndata: {}\n\n", event_type, json)
    }

    fn map_finish_reason(&self, gemini_reason: &str) -> &'static str {
        match gemini_reason {
            "STOP" => "end_turn",
            "MAX_TOKENS" => "max_tokens",
            "SAFETY" => "stop_sequence",  // Best approximation
            "RECITATION" => "stop_sequence",
            _ => "end_turn",
        }
    }
}
```

---

## 5. Pingora Implementation Details

### 5.1 Main Server Configuration

```rust
use pingora::prelude::*;
use pingora::protocols::http::ServerSession;
use std::sync::Arc;

pub struct ProxyConfig {
    pub gemini_api_key: String,
    pub gemini_endpoint: String,  // e.g., "generativelanguage.googleapis.com"
    pub listen_addr: String,
    pub workers: usize,
}

pub fn start_server(config: ProxyConfig) -> Result<()> {
    let mut server = Server::new(Some(Opt {
        upgrade: false,
        daemon: false,
        nocapture: false,
        test: false,
        conf: None,
    }))?;

    server.bootstrap();

    let proxy = ClaudeToGeminiProxy::new(config);
    let proxy_service = http_proxy_service(&server.configuration, proxy);

    server.add_service(proxy_service);
    server.run_forever();
}
```

### 5.2 ProxyHttp Implementation

```rust
use async_trait::async_trait;
use bytes::{Bytes, BytesMut, Buf, BufMut};
use pingora::prelude::*;
use pingora::protocols::http::v1::client::HttpSession;

pub struct ClaudeToGeminiProxy {
    config: Arc<ProxyConfig>,
}

pub struct RequestContext {
    // Transformation state
    transformed_body: Option<Bytes>,
    target_model: String,

    // Streaming parser
    parser: StreamingJsonParser,
    event_generator: SSEEventGenerator,

    // Buffering
    outgoing_events: BytesMut,
}

#[async_trait]
impl ProxyHttp for ClaudeToGeminiProxy {
    type CTX = RequestContext;

    fn new_ctx(&self) -> Self::CTX {
        RequestContext {
            transformed_body: None,
            target_model: "gemini-2.0-flash-exp".to_string(),
            parser: StreamingJsonParser::new(),
            event_generator: SSEEventGenerator::new("gemini-2.0-flash-exp".to_string()),
            outgoing_events: BytesMut::new(),
        }
    }

    async fn upstream_peer(
        &self,
        _session: &mut Session,
        _ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        let peer = Box::new(HttpPeer::new(
            (self.config.gemini_endpoint.as_str(), 443),
            true,  // TLS
            self.config.gemini_endpoint.clone(),  // SNI
        ));

        Ok(peer)
    }

    async fn request_filter(
        &self,
        session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> Result<bool> {
        // Read full request body
        let body_bytes = match session.read_request_body().await? {
            Some(b) => b,
            None => return Err(Error::explain(ErrorType::InvalidHTTPHeader, "Empty request body")),
        };

        // Parse Claude request
        let claude_req: ClaudeRequest = serde_json::from_slice(&body_bytes)
            .map_err(|e| Error::explain(ErrorType::InvalidHTTPHeader, format!("Invalid Claude request: {}", e)))?;

        // Map model
        ctx.target_model = map_model_name(&claude_req.model).to_string();
        ctx.event_generator = SSEEventGenerator::new(ctx.target_model.clone());

        // Transform to Gemini format
        let gemini_req = transform_request(claude_req)?;

        // Serialize
        let transformed = serde_json::to_vec(&gemini_req)
            .map_err(|e| Error::explain(ErrorType::InternalError, format!("Failed to serialize: {}", e)))?;

        ctx.transformed_body = Some(Bytes::from(transformed));

        Ok(false)
    }

    async fn upstream_request_filter(
        &self,
        session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        let header = session.req_header_mut();

        // Rewrite URL
        let new_uri = format!(
            "/v1beta/models/{}:streamGenerateContent?key={}",
            ctx.target_model,
            self.config.gemini_api_key
        );
        header.set_uri(new_uri.parse().unwrap());

        // Set headers
        header.insert_header("Host", &self.config.gemini_endpoint)?;
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
        session: &mut Session,
        _upstream_response: &mut ResponseHeader,
        _ctx: &mut Self::CTX,
    ) -> Result<()> {
        // Set SSE headers
        let header = session.response_written_mut().unwrap();
        header.insert_header("Content-Type", "text/event-stream")?;
        header.insert_header("Cache-Control", "no-cache")?;
        header.insert_header("Connection", "keep-alive")?;
        header.remove_header("Content-Length");  // Chunked encoding

        Ok(())
    }

    async fn response_body_filter(
        &self,
        _session: &mut Session,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> Result<Option<Duration>> {
        // Feed new data to parser
        if let Some(chunk) = body.take() {
            let parsed_chunks = ctx.parser.feed(&chunk);

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
            // Ensure we sent closing events
            if !ctx.event_generator.header_sent {
                log::warn!("Stream ended without any data");
            }
        }

        Ok(None)
    }

    async fn logging(
        &self,
        session: &mut Session,
        _e: Option<&Error>,
        ctx: &mut Self::CTX,
    ) {
        let uri = session.req_header().uri.path();
        let status = session.response_written()
            .map(|r| r.status.as_u16())
            .unwrap_or(0);

        log::info!(
            "Request: {} -> {} (model: {})",
            uri,
            status,
            ctx.target_model
        );
    }
}
```

---

## 6. Error Handling

### 6.1 Error Types

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProxyError {
    #[error("Invalid Claude request: {0}")]
    InvalidClaudeRequest(String),

    #[error("Invalid Gemini response: {0}")]
    InvalidGeminiResponse(String),

    #[error("Model mapping failed: {0}")]
    ModelMappingError(String),

    #[error("Upstream connection failed: {0}")]
    UpstreamConnectionError(String),

    #[error("JSON parsing error: {0}")]
    JsonParseError(#[from] serde_json::Error),

    #[error("Pingora error: {0}")]
    PingoraError(String),
}
```

### 6.2 Error Response Conversion

When Gemini API returns an error (non-200 status), convert to Claude format:

```rust
async fn handle_error_response(
    session: &mut Session,
    upstream_status: u16,
    error_body: &[u8],
) -> Result<Bytes> {
    // Parse Gemini error
    let gemini_error: serde_json::Value = serde_json::from_slice(error_body)
        .unwrap_or_else(|_| json!({
            "error": {
                "message": String::from_utf8_lossy(error_body)
            }
        }));

    // Map to Claude error format
    let claude_error = json!({
        "type": "error",
        "error": {
            "type": map_error_type(upstream_status),
            "message": format!(
                "Gemini API Error: {}",
                gemini_error["error"]["message"]
                    .as_str()
                    .unwrap_or("Unknown error")
            )
        }
    });

    // Return as SSE error event
    let sse = format!(
        "event: error\ndata: {}\n\n",
        serde_json::to_string(&claude_error)?
    );

    Ok(Bytes::from(sse))
}

fn map_error_type(status: u16) -> &'static str {
    match status {
        400 => "invalid_request_error",
        401 | 403 => "authentication_error",
        429 => "rate_limit_error",
        500..=599 => "api_error",
        _ => "api_error",
    }
}
```

---

## 7. Configuration & Deployment

### 7.1 Configuration File

```toml
# config.toml
[server]
listen_addr = "127.0.0.1:8080"
workers = 4

[gemini]
api_key = "${GEMINI_API_KEY}"
endpoint = "generativelanguage.googleapis.com"
# Alternative: Vertex AI endpoint
# endpoint = "us-central1-aiplatform.googleapis.com"

[logging]
level = "info"
format = "json"

[limits]
max_request_body_size = 10485760  # 10MB
connection_timeout_secs = 30
stream_timeout_secs = 300

[tls]
# Optional: for HTTPS termination
enabled = false
cert_path = "/path/to/cert.pem"
key_path = "/path/to/key.pem"
```

### 7.2 Environment Variables

```bash
# Required
export GEMINI_API_KEY="your_gemini_api_key_here"

# Optional
export RUST_LOG=info
export PROXY_LISTEN_ADDR="0.0.0.0:8080"
export PROXY_WORKERS=8
```

### 7.3 Claude Code Integration

Configure Claude Code to use the proxy:

```bash
# Set API endpoint to proxy
export ANTHROPIC_API_URL="http://localhost:8080"

# Use any placeholder key (proxy will replace it)
export ANTHROPIC_API_KEY="placeholder"

# Run Claude Code
claude-code
```

---

## 8. Performance Considerations

### 8.1 Memory Management

**Buffer Pooling:**
```rust
use bytes::BytesMut;

// Pre-allocate buffers to reduce allocations
impl StreamingJsonParser {
    fn new() -> Self {
        Self {
            buffer: BytesMut::with_capacity(8192),  // 8KB initial
            // ...
        }
    }

    fn reset(&mut self) {
        // Reuse buffer instead of dropping
        self.buffer.clear();
        if self.buffer.capacity() > 65536 {  // 64KB max
            self.buffer = BytesMut::with_capacity(8192);
        }
    }
}
```

### 8.2 Zero-Copy Optimizations

```rust
// Use Bytes for reference-counted zero-copy buffers
use bytes::Bytes;

// Avoid copying when possible
fn split_without_copy(buffer: &mut BytesMut, at: usize) -> Bytes {
    buffer.split_to(at).freeze()  // Zero-copy split
}
```

### 8.3 Benchmarking Targets

- **Latency Overhead**: < 5ms added latency vs. direct API call
- **Throughput**: > 1000 requests/second on 4-core machine
- **Memory**: < 50MB baseline + ~1MB per concurrent request
- **CPU**: < 10% overhead for stream parsing

---

## 9. Testing Strategy

### 9.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_mapping() {
        assert_eq!(map_model_name("claude-3-5-sonnet-20241022"), "gemini-2.0-flash-exp");
        assert_eq!(map_model_name("claude-3-opus-20240229"), "gemini-1.5-pro");
    }

    #[test]
    fn test_request_transformation() {
        let claude_req = ClaudeRequest {
            model: "claude-3-5-sonnet".to_string(),
            messages: vec![
                ClaudeMessage {
                    role: "user".to_string(),
                    content: ContentType::Text("Hello".to_string()),
                }
            ],
            system: Some(SystemPrompt::Text("You are helpful".to_string())),
            max_tokens: Some(100),
            stream: true,
            // ...
        };

        let gemini_req = transform_request(claude_req).unwrap();
        assert_eq!(gemini_req.contents.len(), 1);
        assert_eq!(gemini_req.contents[0].role, "user");
        assert!(gemini_req.system_instruction.is_some());
    }

    #[test]
    fn test_streaming_json_parser() {
        let mut parser = StreamingJsonParser::new();

        // Simulate chunked arrival
        let chunk1 = b"[{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"H";
        let chunk2 = b"ello\"}]}}]}";
        let chunk3 = b",{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\" world\"}]}}]}]";

        assert_eq!(parser.feed(chunk1).len(), 0);  // Incomplete
        let results = parser.feed(chunk2);
        assert_eq!(results.len(), 1);  // First complete object
        assert_eq!(results[0].candidates[0].content.as_ref().unwrap().parts[0].text, "Hello");
    }
}
```

### 9.2 Integration Tests

```rust
#[tokio::test]
async fn test_end_to_end_streaming() {
    // Start proxy server
    let proxy = spawn_proxy_server().await;

    // Send Claude-formatted request
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://localhost:{}/v1/messages", proxy.port))
        .header("x-api-key", "test-key")
        .json(&json!({
            "model": "claude-3-5-sonnet",
            "messages": [{"role": "user", "content": "Say hello"}],
            "stream": true,
            "max_tokens": 10
        }))
        .send()
        .await
        .unwrap();

    // Verify SSE stream
    assert_eq!(response.headers()["content-type"], "text/event-stream");

    let mut stream = response.bytes_stream();
    let mut events = Vec::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.unwrap();
        let text = String::from_utf8_lossy(&chunk);
        events.push(text.to_string());
    }

    // Verify event sequence
    assert!(events.iter().any(|e| e.contains("event: message_start")));
    assert!(events.iter().any(|e| e.contains("event: content_block_delta")));
    assert!(events.iter().any(|e| e.contains("event: message_stop")));
}
```

### 9.3 Load Testing

```bash
# Using vegeta
echo "POST http://localhost:8080/v1/messages" | vegeta attack \
  -duration=60s \
  -rate=100/s \
  -body=test_payload.json \
  -header="x-api-key: test" \
  | vegeta report
```

---

## 10. Security Considerations

### 10.1 API Key Handling

- **Never log API keys**: Redact from all logs
- **Environment-based config**: Keys loaded from env vars only
- **No disk storage**: Keys remain in memory only

```rust
use tracing::{info_span, field};

// Redact sensitive fields in logs
info_span!(
    "upstream_request",
    uri = field::debug(uri),
    key = "REDACTED"
);
```

### 10.2 Input Validation

```rust
fn validate_claude_request(req: &ClaudeRequest) -> Result<()> {
    // Check message count
    if req.messages.is_empty() {
        return Err(ProxyError::InvalidClaudeRequest("No messages provided".into()));
    }

    // Validate role alternation
    let mut last_role = None;
    for msg in &req.messages {
        if last_role == Some(&msg.role) && msg.role == "assistant" {
            return Err(ProxyError::InvalidClaudeRequest("Invalid role sequence".into()));
        }
        last_role = Some(&msg.role);
    }

    // Check token limits
    if let Some(max) = req.max_tokens {
        if max > 1_000_000 {
            return Err(ProxyError::InvalidClaudeRequest("max_tokens too large".into()));
        }
    }

    Ok(())
}
```

### 10.3 Rate Limiting

```rust
use std::sync::Arc;
use tokio::sync::Semaphore;

pub struct RateLimiter {
    semaphore: Arc<Semaphore>,
}

impl RateLimiter {
    fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    async fn acquire(&self) -> Result<SemaphorePermit> {
        self.semaphore.acquire().await
            .map_err(|_| ProxyError::RateLimitExceeded)
    }
}
```

---

## 11. Monitoring & Observability

### 11.1 Metrics

```rust
use prometheus::{Counter, Histogram, Registry};

pub struct Metrics {
    pub requests_total: Counter,
    pub request_duration: Histogram,
    pub transformation_errors: Counter,
    pub upstream_errors: Counter,
}

impl Metrics {
    pub fn new(registry: &Registry) -> Self {
        let requests_total = Counter::new("requests_total", "Total requests").unwrap();
        let request_duration = Histogram::new("request_duration_seconds", "Request duration").unwrap();
        // ...

        registry.register(Box::new(requests_total.clone())).unwrap();
        registry.register(Box::new(request_duration.clone())).unwrap();

        Self {
            requests_total,
            request_duration,
            // ...
        }
    }
}
```

### 11.2 Logging

```rust
use tracing::{info, warn, error, debug};

// Structured logging throughout
info!(
    model = %ctx.target_model,
    tokens_in = ctx.event_generator.input_tokens,
    tokens_out = ctx.event_generator.output_tokens,
    "Request completed"
);
```

---

## 12. Future Enhancements

### 12.1 Multi-Model Support

Extend to support multiple LLM providers:

```rust
enum UpstreamProvider {
    Gemini(GeminiConfig),
    OpenAI(OpenAIConfig),
    DeepSeek(DeepSeekConfig),
}

fn route_model(claude_model: &str) -> UpstreamProvider {
    match claude_model {
        m if m.contains("sonnet") => UpstreamProvider::Gemini(/* ... */),
        m if m.contains("opus") => UpstreamProvider::OpenAI(/* ... */),
        _ => UpstreamProvider::Gemini(/* ... */),
    }
}
```

### 12.2 Caching Layer

Implement response caching for identical prompts:

```rust
use pingora::cache::{CacheKey, CacheMeta};

async fn cache_lookup(&self, key: &CacheKey) -> Option<CachedResponse> {
    // Check if we've seen this exact prompt before
    // Return cached SSE stream
}
```

### 12.3 Function Calling Support

Add support for tool/function calling:

```rust
// Transform Claude tool definitions to Gemini function declarations
fn transform_tools(claude_tools: Vec<ClaudeTool>) -> Vec<GeminiFunctionDeclaration> {
    // Map between tool calling formats
}
```

---

## 13. Appendix

### 13.1 Complete File Structure

```
claude-code-proxy/
├── Cargo.toml
├── README.md
├── config.toml
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── proxy.rs              # Main ProxyHttp implementation
│   ├── models/
│   │   ├── mod.rs
│   │   ├── claude.rs         # Claude request/response types
│   │   └── gemini.rs         # Gemini request/response types
│   ├── transform/
│   │   ├── mod.rs
│   │   ├── request.rs        # Request transformation logic
│   │   └── response.rs       # Response transformation logic
│   ├── streaming/
│   │   ├── mod.rs
│   │   ├── parser.rs         # JSON streaming parser
│   │   └── sse.rs            # SSE event generator
│   ├── error.rs              # Error types
│   ├── config.rs             # Configuration loading
│   └── metrics.rs            # Metrics collection
├── tests/
│   ├── integration.rs
│   └── fixtures/
│       ├── claude_request.json
│       └── gemini_response.json
└── benches/
    └── streaming_parser.rs
```

### 13.2 Dependencies Rationale

```toml
[dependencies]
# Core proxy framework
pingora = { version = "0.6", features = ["cache", "lb", "rustls"] }

# Async runtime
tokio = { version = "1.48", features = ["rt-multi-thread", "macros"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Error handling
anyhow = "1.0"
thiserror = "2.0"

# Logging & tracing
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

# Metrics (optional)
prometheus = { version = "0.13", optional = true }

# Utilities
bytes = "1.6"
http = "1.1"
uuid = { version = "1.0", features = ["v4"] }
```

### 13.3 Performance Benchmarks (Target)

| Metric | Target | Measurement Method |
|--------|--------|-------------------|
| Cold start latency | < 100ms | Time to first byte |
| Streaming latency overhead | < 5ms | Proxy vs. direct API |
| Memory per request | < 1MB | Peak allocation |
| Throughput (4 cores) | > 1000 req/s | Concurrent load test |
| JSON parse efficiency | > 500 MB/s | Benchmark suite |

---

## 14. Conclusion

This design specification provides a complete blueprint for implementing a production-ready Claude-to-Gemini protocol translation proxy. The architecture leverages Rust's performance characteristics and Pingora's streaming capabilities to achieve minimal latency overhead while maintaining complete protocol fidelity.

Key implementation challenges addressed:
1. **Streaming JSON parsing** with incomplete chunk handling
2. **SSE event synthesis** with proper sequencing
3. **Request/response mapping** preserving semantic equivalence
4. **Error handling** with transparent error propagation

The modular design enables future extensions to support additional LLM providers, caching layers, and advanced features like function calling.

**Implementation Priority:**
1. Core request/response transformation (Week 1)
2. Streaming parser state machine (Week 1-2)
3. Pingora integration (Week 2)
4. Error handling & logging (Week 3)
5. Testing & optimization (Week 3-4)
