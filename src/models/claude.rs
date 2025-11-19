use serde::{Deserialize, Serialize};

/// Claude Messages API Request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClaudeRequest {
    /// Model identifier (e.g., "claude-3-5-sonnet-20241022")
    pub model: String,

    /// Conversation history with strict user/assistant alternation
    pub messages: Vec<ClaudeMessage>,

    /// Optional system prompt (top-level field)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemPrompt>,

    /// Maximum tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// Temperature (0.0-1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    /// Stop sequences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,

    /// Enable streaming
    #[serde(default)]
    pub stream: bool,

    /// Top-P sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,

    /// Top-K sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClaudeMessage {
    /// "user" or "assistant"
    pub role: String,

    /// Either a string or array of content blocks
    pub content: ContentType,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ContentType {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub block_type: String, // "text", "image", "tool_use", "tool_result"

    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,

    // Additional fields for images, tools, etc.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum SystemPrompt {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// Claude SSE Event Types
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ClaudeSSEEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: MessageMetadata },

    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: u32,
        content_block: ContentBlockMetadata,
    },

    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { index: u32, delta: Delta },

    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: u32 },

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
    Error { error: ErrorInfo },
}

#[derive(Debug, Serialize)]
pub struct MessageMetadata {
    pub id: String,
    #[serde(rename = "type")]
    pub msg_type: String, // "message"
    pub role: String, // "assistant"
    pub model: String,
    pub content: Vec<serde_json::Value>,
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
    pub usage: UsageInfo,
}

#[derive(Debug, Serialize)]
pub struct ContentBlockMetadata {
    #[serde(rename = "type")]
    pub block_type: String, // "text"
    pub text: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum Delta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
}

#[derive(Debug, Serialize)]
pub struct MessageDeltaData {
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageInfo {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Serialize)]
pub struct ErrorInfo {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_request() {
        let json = r#"{
            "model": "claude-3-5-sonnet-20241022",
            "messages": [
                {"role": "user", "content": "Hello"}
            ],
            "max_tokens": 100,
            "stream": true
        }"#;

        let req: ClaudeRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "claude-3-5-sonnet-20241022");
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.max_tokens, Some(100));
        assert!(req.stream);
    }

    #[test]
    fn test_parse_request_with_system() {
        let json = r#"{
            "model": "claude-3-5-sonnet",
            "messages": [
                {"role": "user", "content": "Hello"}
            ],
            "system": "You are a helpful assistant",
            "stream": false
        }"#;

        let req: ClaudeRequest = serde_json::from_str(json).unwrap();
        assert!(matches!(req.system, Some(SystemPrompt::Text(_))));
    }

    #[test]
    fn test_parse_content_blocks() {
        let json = r#"{
            "model": "claude-3-5-sonnet",
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": "Hello"},
                        {"type": "text", "text": "World"}
                    ]
                }
            ]
        }"#;

        let req: ClaudeRequest = serde_json::from_str(json).unwrap();
        match &req.messages[0].content {
            ContentType::Blocks(blocks) => {
                assert_eq!(blocks.len(), 2);
                assert_eq!(blocks[0].block_type, "text");
            }
            _ => panic!("Expected ContentType::Blocks"),
        }
    }
}
