use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

    /// Tool definitions for function calling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ClaudeTool>>,
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

/// Content block with proper tagged enum for type safety
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

/// Claude tool definition
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClaudeTool {
    pub name: String,
    pub description: String,
    pub input_schema: JsonSchema,
}

/// JSON Schema definition (supports OpenAPI-compatible subset)
#[derive(Debug, Clone, Deserialize, Default)]
pub struct JsonSchema {
    #[serde(rename = "type")]
    pub schema_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, Box<JsonSchema>>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "enum")]
    pub enum_values: Option<Vec<serde_json::Value>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<JsonSchema>>,

    // Additional fields we might need
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,

    // Catch-all for additional schema fields from Claude
    // Note: We don't serialize these to Gemini (they're not supported)
    #[serde(flatten, skip_serializing)]
    pub additional: HashMap<String, serde_json::Value>,
}

// Custom Serialize implementation to ensure only supported fields go to Gemini
impl Serialize for JsonSchema {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        let mut map = serializer.serialize_map(None)?;

        map.serialize_entry("type", &self.schema_type)?;

        if let Some(ref desc) = self.description {
            map.serialize_entry("description", desc)?;
        }
        if let Some(ref props) = self.properties {
            map.serialize_entry("properties", props)?;
        }
        if let Some(ref req) = self.required {
            map.serialize_entry("required", req)?;
        }
        if let Some(ref enums) = self.enum_values {
            map.serialize_entry("enum", enums)?;
        }
        if let Some(ref items) = self.items {
            map.serialize_entry("items", items)?;
        }
        if let Some(min) = self.minimum {
            map.serialize_entry("minimum", &min)?;
        }
        if let Some(max) = self.maximum {
            map.serialize_entry("maximum", &max)?;
        }
        if let Some(ref pat) = self.pattern {
            map.serialize_entry("pattern", pat)?;
        }

        map.end()
    }
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
                assert!(matches!(&blocks[0], ContentBlock::Text { .. }));
            }
            _ => panic!("Expected ContentType::Blocks"),
        }
    }

    #[test]
    fn test_parse_tool_use_block() {
        let json = r#"{
            "type": "tool_use",
            "id": "toolu_123",
            "name": "get_weather",
            "input": {"location": "San Francisco"}
        }"#;

        let block: ContentBlock = serde_json::from_str(json).unwrap();
        match block {
            ContentBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "toolu_123");
                assert_eq!(name, "get_weather");
                assert_eq!(input["location"], "San Francisco");
            }
            _ => panic!("Expected ToolUse"),
        }
    }

    #[test]
    fn test_parse_tool_result_block() {
        let json = r#"{
            "type": "tool_result",
            "tool_use_id": "toolu_123",
            "content": "Sunny, 72°F"
        }"#;

        let block: ContentBlock = serde_json::from_str(json).unwrap();
        match block {
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } => {
                assert_eq!(tool_use_id, "toolu_123");
                assert_eq!(content, "Sunny, 72°F");
            }
            _ => panic!("Expected ToolResult"),
        }
    }

    #[test]
    fn test_parse_request_with_tools() {
        let json = r#"{
            "model": "claude-3-5-sonnet",
            "messages": [
                {"role": "user", "content": "What's the weather?"}
            ],
            "tools": [
                {
                    "name": "get_weather",
                    "description": "Get weather for a location",
                    "input_schema": {
                        "type": "object",
                        "properties": {
                            "location": {
                                "type": "string",
                                "description": "City name"
                            }
                        },
                        "required": ["location"]
                    }
                }
            ]
        }"#;

        let req: ClaudeRequest = serde_json::from_str(json).unwrap();
        assert!(req.tools.is_some());
        let tools = req.tools.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "get_weather");
        assert_eq!(tools[0].input_schema.schema_type, "object");
    }
}
