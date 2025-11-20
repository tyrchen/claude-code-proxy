use crate::error::{ProxyError, Result};
use crate::models::claude::{ClaudeRequest, ContentBlock, ContentType, SystemPrompt};
use crate::models::gemini::{
    GeminiContent, GeminiPart, GeminiRequest, GeminiSystemInstruction, GenerationConfig,
};
use crate::state::ConversationState;

/// Extract Gemini parts from Claude content
///
/// Handles text, tool_use, and tool_result blocks.
/// Tool blocks require state tracking which is done externally.
/// Returns (parts, has_non_todo_tool_results)
pub fn extract_parts(
    content: ContentType,
    state: Option<&ConversationState>,
) -> Result<(Vec<GeminiPart>, bool)> {
    let mut has_non_todo_tool_results = false;
    match content {
        ContentType::Text(text) => Ok((vec![GeminiPart::Text { text }], false)),
        ContentType::Blocks(blocks) => {
            let mut parts = Vec::new();
            for block in blocks {
                match block {
                    ContentBlock::Text { text } => {
                        parts.push(GeminiPart::Text { text });
                    }
                    ContentBlock::ToolUse { id, name, input } => {
                        // Tool use blocks appear in assistant messages during multi-turn
                        // SKIP THEM - Gemini doesn't need the model's own function calls in history
                        // Only user messages with functionResponse are needed
                        tracing::debug!(
                            tool_use_id = %id,
                            tool_name = %name,
                            args_empty = input.as_object().is_none_or(|o| o.is_empty()),
                            "Skipping ToolUse block (Gemini doesn't need model's own calls in history)"
                        );
                        // Don't add to parts - this creates empty model messages which get filtered
                    }
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } => {
                        // Transform tool result to function response
                        // Look up the function name from state
                        use crate::models::gemini::FunctionResponse;

                        let function_name = if let Some(state) = state {
                            state.get_function_name(&tool_use_id).unwrap_or_else(|| {
                                tracing::warn!(
                                    tool_use_id = %tool_use_id,
                                    "No function name found in state, using tool_use_id as fallback"
                                );
                                tool_use_id.clone()
                            })
                        } else {
                            tracing::warn!(
                                "No state provided for tool result transformation, using tool_use_id as function name"
                            );
                            tool_use_id.clone()
                        };

                        // Track if we have non-TodoWrite tool results (for auto_todo_prompt feature)
                        if function_name != "TodoWrite" {
                            has_non_todo_tool_results = true;
                        }

                        parts.push(GeminiPart::FunctionResponse {
                            function_response: FunctionResponse {
                                name: function_name,
                                response: serde_json::json!({
                                    "result": content,
                                    "error": is_error.unwrap_or(false)
                                }),
                            },
                        });
                    }
                }
            }
            Ok((parts, has_non_todo_tool_results))
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
                .filter_map(|b| match b {
                    ContentBlock::Text { text } => Some(GeminiPart::Text { text }),
                    _ => None, // Skip non-text blocks in system prompt
                })
                .collect(),
        };
        GeminiSystemInstruction { parts }
    })
}

/// Transform Claude request to Gemini request
///
/// Optionally accepts a ConversationState for tool result handling.
pub fn transform_request(claude_req: ClaudeRequest) -> Result<GeminiRequest> {
    transform_request_with_state(claude_req, None, false)
}

/// Transform Claude request to Gemini request with state tracking
pub fn transform_request_with_state(
    claude_req: ClaudeRequest,
    state: Option<&ConversationState>,
    auto_todo_prompt: bool,
) -> Result<GeminiRequest> {
    // 1. Convert messages
    // For tool use: only keep first user message + most recent user message with tool results
    // This prevents accumulation of duplicate tool results across turns
    let mut contents = Vec::new();
    let mut has_non_todo_tool_results = false;
    tracing::info!(
        "Processing {} messages from Claude",
        claude_req.messages.len()
    );

    // Find the first user message and the last user message (which should have tool results)
    let first_user_idx = claude_req.messages.iter().position(|m| m.role == "user");
    let last_user_idx = claude_req.messages.iter().rposition(|m| m.role == "user");

    for (idx, msg) in claude_req.messages.iter().enumerate() {
        // For long conversations with tool use, only keep:
        // - First user message (the original request)
        // - Last user message (the most recent tool results)
        // Skip intermediate user messages that contain old tool results
        if msg.role == "user"
            && claude_req.messages.len() > 3
            && Some(idx) != first_user_idx
            && Some(idx) != last_user_idx
        {
            tracing::debug!("Skipping intermediate user message (old tool results)");
            continue;
        }

        let role = match msg.role.as_str() {
            "assistant" => "model",
            "user" => "user",
            _ => {
                return Err(ProxyError::TransformationError(format!(
                    "Invalid role: {}",
                    msg.role
                )));
            }
        };

        let (parts, msg_has_tool_results) = extract_parts(msg.content.clone(), state)?;

        // Track if this message has non-TodoWrite tool results
        if msg_has_tool_results {
            has_non_todo_tool_results = true;
        }

        // Skip empty messages (can happen when all tool results are TodoWrite or all blocks are ToolUse)
        if parts.is_empty() {
            tracing::debug!(
                role = %role,
                "Skipping empty message (no content after filtering)"
            );
            continue;
        }

        contents.push(GeminiContent {
            role: Some(role.to_string()),
            parts,
        });
    }

    tracing::info!(
        "Sending {} messages to Gemini (after filtering)",
        contents.len()
    );

    // Inject todo update requirement if auto_todo_prompt is enabled and we have tool results
    if auto_todo_prompt && has_non_todo_tool_results {
        tracing::info!("Adding mandatory todo update instruction after tool results");

        // Find the last user message and append the directive
        if let Some(last_msg) = contents
            .iter_mut()
            .rev()
            .find(|c| c.role == Some("user".to_string()))
        {
            last_msg.parts.push(GeminiPart::Text {
                text: "\n\n<system-reminder>\nYou MUST now call the TodoWrite tool to update your task list based on the tool execution results above. Analyze the results and:\n\
                       1. Mark the current task as 'completed' if the tool successfully finished it\n\
                       2. Keep it as 'in_progress' if more work is needed\n\
                       Call TodoWrite now with the updated task list before continuing.\n\
                       </system-reminder>".to_string(),
            });
        }
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

    // 4. Transform tools if present
    let tools = claude_req
        .tools
        .map(crate::transform::tools::transform_tools)
        .transpose()?;

    Ok(GeminiRequest {
        contents,
        system_instruction,
        generation_config,
        safety_settings: None, // Use Gemini defaults
        tools,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::claude::{ClaudeMessage, ContentBlock};

    #[test]
    fn test_extract_parts_text() {
        let content = ContentType::Text("Hello world".to_string());
        let (parts, has_tool_results) = extract_parts(content, None).unwrap();

        assert_eq!(parts.len(), 1);
        assert!(!has_tool_results);
        match &parts[0] {
            GeminiPart::Text { text } => assert_eq!(text, "Hello world"),
            _ => panic!("Expected Text part"),
        }
    }

    #[test]
    fn test_extract_parts_blocks() {
        let blocks = vec![
            ContentBlock::Text {
                text: "First".to_string(),
            },
            ContentBlock::Text {
                text: "Second".to_string(),
            },
        ];
        let content = ContentType::Blocks(blocks);
        let (parts, has_tool_results) = extract_parts(content, None).unwrap();

        assert_eq!(parts.len(), 2);
        assert!(!has_tool_results);
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
            tools: None,
        };

        let gemini_req = transform_request(claude_req).unwrap();

        assert_eq!(gemini_req.contents.len(), 1);
        assert_eq!(gemini_req.contents[0].role, Some("user".to_string()));
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
            tools: None,
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
            tools: None,
        };

        let gemini_req = transform_request(claude_req).unwrap();

        assert_eq!(gemini_req.contents.len(), 2);
        assert_eq!(gemini_req.contents[0].role, Some("user".to_string()));
        assert_eq!(gemini_req.contents[1].role, Some("model".to_string())); // assistant -> model
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
            tools: None,
        };

        let result = transform_request(claude_req);
        assert!(result.is_err());
    }
}
