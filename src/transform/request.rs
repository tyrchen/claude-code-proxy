use crate::error::{ProxyError, Result};
use crate::models::claude::{ClaudeRequest, ContentType, SystemPrompt};
use crate::models::gemini::{
    GeminiContent, GeminiPart, GeminiRequest, GeminiSystemInstruction, GenerationConfig,
};

/// Extract Gemini parts from Claude content
pub fn extract_parts(content: ContentType) -> Result<Vec<GeminiPart>> {
    match content {
        ContentType::Text(text) => Ok(vec![GeminiPart::Text { text }]),
        ContentType::Blocks(blocks) => {
            let mut parts = Vec::new();
            for block in blocks {
                match block.block_type.as_str() {
                    "text" => {
                        if let Some(text) = block.text {
                            parts.push(GeminiPart::Text { text });
                        }
                    }
                    _ => {
                        // Log warning for unsupported types
                        eprintln!("Warning: Unsupported block type: {}", block.block_type);
                    }
                }
            }
            Ok(parts)
        }
    }
}

/// Convert Claude system prompt to Gemini system instruction
pub fn convert_system_prompt(system: Option<SystemPrompt>) -> Option<GeminiSystemInstruction> {
    system.map(|sys| {
        let parts = match sys {
            SystemPrompt::Text(text) => vec![GeminiPart::Text { text }],
            SystemPrompt::Blocks(blocks) => blocks
                .into_iter()
                .filter_map(|b| b.text.map(|text| GeminiPart::Text { text }))
                .collect(),
        };
        GeminiSystemInstruction { parts }
    })
}

/// Transform Claude request to Gemini request
pub fn transform_request(claude_req: ClaudeRequest) -> Result<GeminiRequest> {
    // 1. Convert messages
    let mut contents = Vec::new();
    for msg in claude_req.messages {
        let role = match msg.role.as_str() {
            "assistant" => "model", // CRITICAL: role name change
            "user" => "user",
            _ => {
                return Err(ProxyError::TransformationError(format!(
                    "Invalid role: {}",
                    msg.role
                )))
            }
        };

        let parts = extract_parts(msg.content)?;

        contents.push(GeminiContent {
            role: role.to_string(),
            parts,
        });
    }

    // 2. Convert system prompt
    let system_instruction = convert_system_prompt(claude_req.system);

    // 3. Build generation config
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
        safety_settings: None, // Use Gemini defaults
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::claude::{ClaudeMessage, ContentBlock};

    #[test]
    fn test_extract_parts_text() {
        let content = ContentType::Text("Hello world".to_string());
        let parts = extract_parts(content).unwrap();

        assert_eq!(parts.len(), 1);
        match &parts[0] {
            GeminiPart::Text { text } => assert_eq!(text, "Hello world"),
            _ => panic!("Expected Text part"),
        }
    }

    #[test]
    fn test_extract_parts_blocks() {
        let blocks = vec![
            ContentBlock {
                block_type: "text".to_string(),
                text: Some("First".to_string()),
                extra: serde_json::json!({}),
            },
            ContentBlock {
                block_type: "text".to_string(),
                text: Some("Second".to_string()),
                extra: serde_json::json!({}),
            },
        ];
        let content = ContentType::Blocks(blocks);
        let parts = extract_parts(content).unwrap();

        assert_eq!(parts.len(), 2);
    }

    #[test]
    fn test_convert_system_prompt_text() {
        let system = Some(SystemPrompt::Text("You are helpful".to_string()));
        let instruction = convert_system_prompt(system).unwrap();

        assert_eq!(instruction.parts.len(), 1);
        match &instruction.parts[0] {
            GeminiPart::Text { text } => assert_eq!(text, "You are helpful"),
            _ => panic!("Expected Text part"),
        }
    }

    #[test]
    fn test_convert_system_prompt_none() {
        let instruction = convert_system_prompt(None);
        assert!(instruction.is_none());
    }

    #[test]
    fn test_transform_request_simple() {
        let claude_req = ClaudeRequest {
            model: "claude-3-5-sonnet-20241022".to_string(),
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: ContentType::Text("Hello".to_string()),
            }],
            system: None,
            max_tokens: Some(100),
            temperature: None,
            stop_sequences: None,
            stream: true,
            top_p: None,
            top_k: None,
        };

        let gemini_req = transform_request(claude_req).unwrap();

        assert_eq!(gemini_req.contents.len(), 1);
        assert_eq!(gemini_req.contents[0].role, "user");
        assert_eq!(gemini_req.contents[0].parts.len(), 1);
        assert!(gemini_req.system_instruction.is_none());
        assert_eq!(
            gemini_req
                .generation_config
                .as_ref()
                .unwrap()
                .max_output_tokens,
            Some(100)
        );
    }

    #[test]
    fn test_transform_request_with_system() {
        let claude_req = ClaudeRequest {
            model: "claude-3-5-sonnet".to_string(),
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: ContentType::Text("What is Rust?".to_string()),
            }],
            system: Some(SystemPrompt::Text("You are a Rust expert".to_string())),
            max_tokens: Some(500),
            temperature: Some(0.7),
            stop_sequences: None,
            stream: true,
            top_p: None,
            top_k: None,
        };

        let gemini_req = transform_request(claude_req).unwrap();

        assert!(gemini_req.system_instruction.is_some());
        assert_eq!(
            gemini_req.generation_config.as_ref().unwrap().temperature,
            Some(0.7)
        );
    }

    #[test]
    fn test_transform_request_role_mapping() {
        let claude_req = ClaudeRequest {
            model: "claude-3-5-sonnet".to_string(),
            messages: vec![
                ClaudeMessage {
                    role: "user".to_string(),
                    content: ContentType::Text("Hello".to_string()),
                },
                ClaudeMessage {
                    role: "assistant".to_string(),
                    content: ContentType::Text("Hi there!".to_string()),
                },
            ],
            system: None,
            max_tokens: Some(100),
            temperature: None,
            stop_sequences: None,
            stream: true,
            top_p: None,
            top_k: None,
        };

        let gemini_req = transform_request(claude_req).unwrap();

        assert_eq!(gemini_req.contents.len(), 2);
        assert_eq!(gemini_req.contents[0].role, "user");
        assert_eq!(gemini_req.contents[1].role, "model"); // assistant -> model
    }

    #[test]
    fn test_transform_request_invalid_role() {
        let claude_req = ClaudeRequest {
            model: "claude-3-5-sonnet".to_string(),
            messages: vec![ClaudeMessage {
                role: "invalid".to_string(),
                content: ContentType::Text("Hello".to_string()),
            }],
            system: None,
            max_tokens: Some(100),
            temperature: None,
            stop_sequences: None,
            stream: true,
            top_p: None,
            top_k: None,
        };

        let result = transform_request(claude_req);
        assert!(result.is_err());
    }
}
