use claude_code_proxy::{
    models::{
        claude::{ClaudeMessage, ClaudeRequest, ClaudeTool, ContentBlock, ContentType, JsonSchema},
        gemini::{Candidate, GeminiContent, GeminiPart, GeminiStreamChunk},
    },
    state::ConversationState,
    streaming::SSEEventGenerator,
    transform::{transform_request_with_state, validate_claude_request},
};
use serde_json::json;

/// Test round-trip transformation: Claude → Gemini → SSE → Claude
#[test]
fn test_todowrite_roundtrip() {
    // Step 1: Create a Claude request with TodoWrite tool
    let claude_req = ClaudeRequest {
        model: "claude-sonnet-4-5-20250929".to_string(),
        max_tokens: Some(1024),
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ContentType::Text("Create a todo list".to_string()),
        }],
        tools: Some(vec![ClaudeTool {
            name: "TodoWrite".to_string(),
            description: "Manage todo list".to_string(),
            input_schema: JsonSchema {
                schema_type: "object".to_string(),
                properties: None,
                required: Some(vec!["todos".to_string()]),
                ..Default::default()
            },
        }]),
        system: None,
        temperature: None,
        stop_sequences: None,
        top_p: None,
        top_k: None,
        stream: true,
    };

    // Validate the request
    assert!(validate_claude_request(&claude_req).is_ok());

    // Step 2: Transform to Gemini format
    let state = ConversationState::new();
    let gemini_req = transform_request_with_state(claude_req.clone(), Some(&state), false).unwrap();

    // Verify transformation
    assert!(!gemini_req.contents.is_empty());
    assert_eq!(
        gemini_req
            .generation_config
            .as_ref()
            .and_then(|c| c.max_output_tokens),
        Some(1024)
    );

    // Verify tool was transformed
    assert!(gemini_req.tools.is_some());
    let tools = gemini_req.tools.as_ref().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].function_declarations.len(), 1);
    assert_eq!(tools[0].function_declarations[0].name, "TodoWrite");

    // Step 3: Simulate Gemini response with function call
    let gemini_response_json = json!({
        "candidates": [{
            "content": {
                "parts": [{
                    "functionCall": {
                        "name": "TodoWrite",
                        "args": {
                            "todos": [{
                                "content": "Test task",
                                "status": "pending",
                                "activeForm": "Testing task"
                            }]
                        }
                    }
                }]
            },
            "finishReason": "STOP"
        }]
    });

    // Step 4: Parse as streaming chunk
    let gemini_chunk = serde_json::from_value(gemini_response_json).unwrap();

    // Step 5: Transform back to Claude SSE
    let mut sse_gen =
        SSEEventGenerator::with_state("gemini-3-pro-preview".to_string(), state.clone());
    let events = sse_gen.generate_events(gemini_chunk);

    // Verify SSE events generated
    assert!(!events.is_empty());

    // Check for tool_use event
    let has_tool_use = events.iter().any(|e| e.contains("tool_use"));
    assert!(has_tool_use, "Should have tool_use event");

    // Check for stop_reason: tool_use
    let has_tool_use_stop = events
        .iter()
        .any(|e| e.contains("\"stop_reason\":\"tool_use\""));
    assert!(has_tool_use_stop, "Should have tool_use stop reason");

    // Step 6: Extract tool_use_id from SSE events
    let mut tool_use_id = None;
    for event in &events {
        if event.contains("tool_use") && event.contains("\"id\":") {
            // Parse the event to extract ID
            if let Some(data_start) = event.find("data: ") {
                let data_str = &event[data_start + 6..];
                if let Some(line_end) = data_str.find('\n') {
                    let json_str = &data_str[..line_end];
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str)
                        && let Some(id) = parsed
                            .pointer("/content_block/id")
                            .or_else(|| parsed.pointer("/delta/tool_use/id"))
                            .and_then(|v| v.as_str())
                    {
                        tool_use_id = Some(id.to_string());
                        break;
                    }
                }
            }
        }
    }

    assert!(tool_use_id.is_some(), "Should extract tool_use_id from SSE");
    let extracted_id = tool_use_id.unwrap();

    // Step 7: Verify state mapping was registered
    let function_name = state.get_function_name(&extracted_id);
    assert_eq!(function_name, Some("TodoWrite".to_string()));

    // Step 8: Verify round-trip check
    assert!(state.verify_round_trip(&extracted_id));

    // Step 9: Create tool_result response
    let tool_result_req = ClaudeRequest {
        model: "claude-sonnet-4-5-20250929".to_string(),
        max_tokens: Some(1024),
        messages: vec![
            ClaudeMessage {
                role: "user".to_string(),
                content: ContentType::Text("Create a todo list".to_string()),
            },
            ClaudeMessage {
                role: "assistant".to_string(),
                content: ContentType::Blocks(vec![ContentBlock::ToolUse {
                    id: extracted_id.clone(),
                    name: "TodoWrite".to_string(),
                    input: json!({
                        "todos": [{
                            "content": "Test task",
                            "status": "pending",
                            "activeForm": "Testing task"
                        }]
                    }),
                }]),
            },
            ClaudeMessage {
                role: "user".to_string(),
                content: ContentType::Blocks(vec![ContentBlock::ToolResult {
                    tool_use_id: extracted_id.clone(),
                    content: "Todos have been modified successfully".to_string(),
                    is_error: Some(false),
                }]),
            },
        ],
        tools: Some(vec![ClaudeTool {
            name: "TodoWrite".to_string(),
            description: "Manage todo list".to_string(),
            input_schema: JsonSchema {
                schema_type: "object".to_string(),
                properties: None,
                required: Some(vec!["todos".to_string()]),
                ..Default::default()
            },
        }]),
        system: None,
        temperature: None,
        stop_sequences: None,
        top_p: None,
        top_k: None,
        stream: true,
    };

    // Step 10: Transform again with tool_result
    let second_gemini = transform_request_with_state(tool_result_req, Some(&state), false).unwrap();

    // Step 11: Verify function response was created
    let has_function_response = second_gemini.contents.iter().any(|content| {
        content.parts.iter().any(|part| {
            matches!(
                part,
                claude_code_proxy::models::gemini::GeminiPart::FunctionResponse { .. }
            )
        })
    });

    assert!(
        has_function_response,
        "Should have function_response after tool_result"
    );
}

/// Test partial streaming JSON buffering
#[test]
fn test_partial_function_call_streaming() {
    use claude_code_proxy::streaming::StreamingJsonParser;

    let mut parser = StreamingJsonParser::new();

    // Simulate chunked Gemini response with function call
    let chunks = vec![
        br#"[{"candidates":[{"content":{"parts":[{"functionCall":{"name":"TodoWrite","#.to_vec(),
        br#""args":{"todos":[{"content":"Test","status":"pending","#.to_vec(),
        br#""activeForm":"Testing"}]}}}]}}]}]"#.to_vec(),
    ];

    let mut all_chunks = Vec::new();
    for chunk in chunks {
        let parsed = parser.feed(&chunk).unwrap();
        all_chunks.extend(parsed);
    }

    // Should successfully parse despite being chunked
    assert!(!all_chunks.is_empty());

    // Verify we got a valid chunk with function call
    let has_function_call = all_chunks.iter().any(|chunk| {
        chunk.candidates.first().is_some_and(|c| {
            c.content.as_ref().is_some_and(|content| {
                content.parts.iter().any(|part| {
                    matches!(
                        part,
                        GeminiPart::FunctionCall { .. }
                            | GeminiPart::FunctionCallWithThought { .. }
                    )
                })
            })
        })
    });

    assert!(
        has_function_call,
        "Should parse function call from chunked data"
    );
}

/// Test conversation context tracking
#[test]
fn test_conversation_context_tracking() {
    let state = ConversationState::new();

    // Register multiple tool calls with context
    state.register_tool_use_with_context(
        "toolu_1".to_string(),
        "TodoWrite".to_string(),
        None,
        json!({"todos": []}),
        Some("conv_123".to_string()),
    );

    state.register_tool_use_with_context(
        "toolu_2".to_string(),
        "Bash".to_string(),
        None,
        json!({"command": "ls"}),
        Some("conv_123".to_string()),
    );

    state.register_tool_use_with_context(
        "toolu_3".to_string(),
        "Read".to_string(),
        None,
        json!({"file_path": "/test"}),
        Some("conv_456".to_string()),
    );

    // Test conversation filtering
    let conv_123_tools = state.get_by_conversation("conv_123");
    assert_eq!(conv_123_tools.len(), 2);

    let conv_456_tools = state.get_by_conversation("conv_456");
    assert_eq!(conv_456_tools.len(), 1);

    // Test request index ordering
    let sorted = state.get_sorted_by_request_index();
    assert_eq!(sorted.len(), 3);
    // Should be in order: toolu_1, toolu_2, toolu_3
    assert_eq!(sorted[0].1.request_index, 0);
    assert_eq!(sorted[1].1.request_index, 1);
    assert_eq!(sorted[2].1.request_index, 2);

    // Test round-trip verification
    assert!(state.verify_round_trip("toolu_1"));
    assert!(state.verify_round_trip("toolu_2"));
    assert!(!state.verify_round_trip("nonexistent"));
}

/// Test stop reason intelligence
#[test]
fn test_stop_reason_context_awareness() {
    let state = ConversationState::new();
    let mut sse_gen = SSEEventGenerator::with_state("gemini-3-pro-preview".to_string(), state);

    // Test 1: Function call should result in tool_use stop reason
    let function_call_chunk = GeminiStreamChunk {
        candidates: vec![Candidate {
            content: Some(GeminiContent {
                parts: vec![GeminiPart::FunctionCall {
                    function_call: claude_code_proxy::models::gemini::FunctionCall {
                        name: "TodoWrite".to_string(),
                        args: json!({
                            "todos": [{
                                "content": "Test task",
                                "status": "pending",
                                "activeForm": "Testing task"
                            }]
                        }),
                    },
                }],
                role: Some("model".to_string()),
            }),
            finish_reason: Some("STOP".to_string()),
            safety_ratings: None,
            index: None,
        }],
        usage_metadata: None,
        prompt_feedback: None,
    };

    let events = sse_gen.generate_events(function_call_chunk);
    let has_tool_use_stop = events
        .iter()
        .any(|e| e.contains("\"stop_reason\":\"tool_use\""));
    assert!(
        has_tool_use_stop,
        "Function call should have tool_use stop reason"
    );

    // Test 2: Regular text should use mapped finish reason
    let text_chunk = GeminiStreamChunk {
        candidates: vec![Candidate {
            content: Some(GeminiContent {
                parts: vec![GeminiPart::Text {
                    text: "Hello".to_string(),
                }],
                role: Some("model".to_string()),
            }),
            finish_reason: Some("MAX_TOKENS".to_string()),
            safety_ratings: None,
            index: None,
        }],
        usage_metadata: None,
        prompt_feedback: None,
    };

    let mut sse_gen2 = SSEEventGenerator::new("gemini-3-pro-preview".to_string());
    let events2 = sse_gen2.generate_events(text_chunk);
    let has_max_tokens = events2
        .iter()
        .any(|e| e.contains("\"stop_reason\":\"max_tokens\""));
    assert!(has_max_tokens, "MAX_TOKENS should map to max_tokens");
}
