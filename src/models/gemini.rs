use serde::{Deserialize, Serialize};

/// Gemini GenerateContent Request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiRequest {
    /// Conversation history
    pub contents: Vec<GeminiContent>,

    /// System instructions (wrapped in special structure)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<GeminiSystemInstruction>,

    /// Generation parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<GenerationConfig>,

    /// Safety settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_settings: Option<Vec<SafetySetting>>,

    /// Tool/function declarations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<GeminiTool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiContent {
    /// "user" or "model" (NOT "assistant")
    /// Optional because Gemini may return empty content on errors
    #[serde(default)]
    pub role: Option<String>,

    /// Always an array, even for single text
    /// Optional because Gemini may return empty content on errors
    #[serde(default)]
    pub parts: Vec<GeminiPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiSystemInstruction {
    pub parts: Vec<GeminiPart>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GeminiPart {
    Text {
        text: String,
    },
    // Gemini 3 Pro Preview includes thinking signature - we ignore it
    TextWithThought {
        text: String,
        #[serde(rename = "thoughtSignature")]
        thought_signature: String,
    },
    InlineData {
        inline_data: InlineData,
    },
    // Function call WITH thought signature (Gemini 3 Pro)
    FunctionCallWithThought {
        #[serde(rename = "functionCall")]
        function_call: FunctionCall,
        #[serde(rename = "thoughtSignature")]
        thought_signature: String,
    },
    // Function call WITHOUT thought signature (regular or parallel calls after first)
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: FunctionCall,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: FunctionResponse,
    },
}

/// Gemini function call (output from model)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionCall {
    pub name: String,
    pub args: serde_json::Value,
}

/// Gemini function response (input to model with execution results)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionResponse {
    pub name: String,
    pub response: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineData {
    pub mime_type: String,
    pub data: String, // base64
}

/// Tool declaration wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiTool {
    pub function_declarations: Vec<GeminiFunctionDeclaration>,
}

/// Individual function declaration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiFunctionDeclaration {
    pub name: String,
    pub description: String,
    pub parameters: crate::models::claude::JsonSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetySetting {
    pub category: String,
    pub threshold: String,
}

/// Gemini Streaming Response
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiStreamChunk {
    #[serde(default)]
    pub candidates: Vec<Candidate>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_metadata: Option<UsageMetadata>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_feedback: Option<PromptFeedback>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<GeminiContent>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>, // "STOP", "MAX_TOKENS", "SAFETY", etc.

    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_ratings: Option<Vec<SafetyRating>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageMetadata {
    pub prompt_token_count: Option<u32>,
    pub candidates_token_count: Option<u32>,
    pub total_token_count: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptFeedback {
    pub block_reason: Option<String>,
    pub safety_ratings: Option<Vec<SafetyRating>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SafetyRating {
    pub category: String,
    pub probability: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_gemini_request() {
        let req = GeminiRequest {
            contents: vec![GeminiContent {
                role: Some("user".to_string()),
                parts: vec![GeminiPart::Text {
                    text: "Hello".to_string(),
                }],
            }],
            system_instruction: None,
            generation_config: Some(GenerationConfig {
                max_output_tokens: Some(100),
                temperature: None,
                top_p: None,
                top_k: None,
                stop_sequences: None,
            }),
            safety_settings: None,
            tools: None,
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("maxOutputTokens"));
        assert!(json.contains("contents"));
    }

    #[test]
    fn test_parse_gemini_stream_chunk() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "Hello"}],
                    "role": "model"
                }
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 1
            }
        }"#;

        let chunk: GeminiStreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.candidates.len(), 1);
        assert_eq!(
            chunk.usage_metadata.as_ref().unwrap().prompt_token_count,
            Some(10)
        );
    }

    #[test]
    fn test_parse_gemini_finish_chunk() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "!"}],
                    "role": "model"
                },
                "finishReason": "STOP"
            }]
        }"#;

        let chunk: GeminiStreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.candidates[0].finish_reason.as_ref().unwrap(), "STOP");
    }

    #[test]
    fn test_serialize_gemini_request_with_tools() {
        use crate::models::claude::JsonSchema;
        use std::collections::HashMap;

        let mut properties = HashMap::new();
        properties.insert(
            "location".to_string(),
            Box::new(JsonSchema {
                schema_type: "string".to_string(),
                description: Some("City name".to_string()),
                ..Default::default()
            }),
        );

        let req = GeminiRequest {
            contents: vec![GeminiContent {
                role: Some("user".to_string()),
                parts: vec![GeminiPart::Text {
                    text: "What's the weather?".to_string(),
                }],
            }],
            system_instruction: None,
            generation_config: None,
            safety_settings: None,
            tools: Some(vec![GeminiTool {
                function_declarations: vec![GeminiFunctionDeclaration {
                    name: "get_weather".to_string(),
                    description: "Get weather for a location".to_string(),
                    parameters: JsonSchema {
                        schema_type: "object".to_string(),
                        properties: Some(properties),
                        required: Some(vec!["location".to_string()]),
                        ..Default::default()
                    },
                }],
            }]),
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("functionDeclarations"));
        assert!(json.contains("get_weather"));
    }

    #[test]
    fn test_parse_function_call() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "parts": [{
                        "functionCall": {
                            "name": "get_weather",
                            "args": {"location": "San Francisco"}
                        }
                    }],
                    "role": "model"
                }
            }]
        }"#;

        let chunk: GeminiStreamChunk = serde_json::from_str(json).unwrap();
        let part = &chunk.candidates[0].content.as_ref().unwrap().parts[0];

        match part {
            GeminiPart::FunctionCall { function_call } => {
                assert_eq!(function_call.name, "get_weather");
                assert_eq!(function_call.args["location"], "San Francisco");
            }
            _ => panic!("Expected FunctionCall"),
        }
    }

    #[test]
    fn test_serialize_function_call_with_thought() {
        let part = GeminiPart::FunctionCallWithThought {
            function_call: FunctionCall {
                name: "test_tool".to_string(),
                args: serde_json::json!({"param": "value"}),
            },
            thought_signature: "test_signature_123".to_string(),
        };

        let json = serde_json::to_string(&part).unwrap();
        println!("Serialized: {}", json);

        // Verify structure
        let obj: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(obj["functionCall"]["name"], "test_tool");
        assert_eq!(obj["thoughtSignature"], "test_signature_123");
    }
}
