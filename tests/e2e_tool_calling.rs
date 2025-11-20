/// End-to-end integration tests for complete tool calling flow
///
/// These tests simulate the full conversation lifecycle including:
/// - Request with tools -> Gemini
/// - Gemini function call -> Claude tool_use
/// - Claude tool_result -> Gemini function response
/// - Final response
use claude_code_proxy::models::claude::*;
use claude_code_proxy::models::gemini::*;
use claude_code_proxy::state::ConversationState;
use claude_code_proxy::transform::*;
use std::collections::HashMap;

/// Simulate complete TodoWrite tool flow (the original failing case)
#[test]
fn test_e2e_todo_write_flow() {
    let state = ConversationState::new();

    // Build TodoWrite tool schema
    let mut todo_item_props = HashMap::new();
    todo_item_props.insert(
        "content".to_string(),
        Box::new(JsonSchema {
            schema_type: "string".to_string(),
            description: Some("Task description".to_string()),
            ..Default::default()
        }),
    );
    todo_item_props.insert(
        "status".to_string(),
        Box::new(JsonSchema {
            schema_type: "string".to_string(),
            enum_values: Some(vec![
                serde_json::json!("pending"),
                serde_json::json!("in_progress"),
                serde_json::json!("completed"),
            ]),
            ..Default::default()
        }),
    );
    todo_item_props.insert(
        "activeForm".to_string(),
        Box::new(JsonSchema {
            schema_type: "string".to_string(),
            description: Some("Present continuous form".to_string()),
            ..Default::default()
        }),
    );

    let mut properties = HashMap::new();
    properties.insert(
        "todos".to_string(),
        Box::new(JsonSchema {
            schema_type: "array".to_string(),
            items: Some(Box::new(JsonSchema {
                schema_type: "object".to_string(),
                properties: Some(todo_item_props),
                required: Some(vec![
                    "content".to_string(),
                    "status".to_string(),
                    "activeForm".to_string(),
                ]),
                ..Default::default()
            })),
            ..Default::default()
        }),
    );

    let todo_tool = ClaudeTool {
        name: "TodoWrite".to_string(),
        description: "Create and manage task lists".to_string(),
        input_schema: JsonSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["todos".to_string()]),
            ..Default::default()
        },
    };

    // Turn 1: User asks to create todos
    let turn1 = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ContentType::Text("Review the code carefully and update readme".to_string()),
        }],
        system: None,
        max_tokens: Some(4096),
        temperature: None,
        stop_sequences: None,
        stream: true,
        top_p: None,
        top_k: None,
        tools: Some(vec![todo_tool]),
    };

    let gemini_req1 = transform_request_with_state(turn1, Some(&state), false).unwrap();

    // Verify tools transformed
    assert!(gemini_req1.tools.is_some());
    let tools = gemini_req1.tools.unwrap();
    assert_eq!(tools[0].function_declarations[0].name, "TodoWrite");

    // Simulate Gemini's response with function call
    let gemini_function_call = FunctionCall {
        name: "TodoWrite".to_string(),
        args: serde_json::json!({
            "todos": [
                {
                    "content": "Review code for issues",
                    "status": "pending",
                    "activeForm": "Reviewing code"
                },
                {
                    "content": "Update README with findings",
                    "status": "pending",
                    "activeForm": "Updating README"
                }
            ]
        }),
    };

    let (tool_use_id, tool_use_block) = transform_function_call(&gemini_function_call).unwrap();

    // Register in state (would be done by SSEEventGenerator)
    state.register_tool_use(
        tool_use_id.clone(),
        "TodoWrite".to_string(),
        Some("test_signature_123".to_string()),
        gemini_function_call.args.clone(),
    );

    // Verify tool_use block
    let tool_use_block_clone = match &tool_use_block {
        ContentBlock::ToolUse { id, name, input } => {
            assert_eq!(id, &tool_use_id);
            assert_eq!(name, "TodoWrite");
            assert!(input["todos"].is_array());
            assert_eq!(input["todos"].as_array().unwrap().len(), 2);
            tool_use_block.clone()
        }
        _ => panic!("Expected ToolUse"),
    };

    // Turn 2: Claude Code executes TodoWrite and sends result
    let turn2 = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![
            ClaudeMessage {
                role: "user".to_string(),
                content: ContentType::Text(
                    "Review the code carefully and update readme".to_string(),
                ),
            },
            ClaudeMessage {
                role: "assistant".to_string(),
                content: ContentType::Blocks(vec![tool_use_block_clone]),
            },
            ClaudeMessage {
                role: "user".to_string(),
                content: ContentType::Blocks(vec![ContentBlock::ToolResult {
                    tool_use_id: tool_use_id.clone(),
                    content: "Todos have been added successfully".to_string(),
                    is_error: None,
                }]),
            },
        ],
        system: None,
        max_tokens: Some(4096),
        temperature: None,
        stop_sequences: None,
        stream: true,
        top_p: None,
        top_k: None,
        tools: None,
    };

    let gemini_req2 = transform_request_with_state(turn2, Some(&state), false).unwrap();

    // Verify the conversation flow - model messages with only ToolUse are skipped
    // We should have 2 messages: initial user request + user message with TodoWrite result
    assert_eq!(gemini_req2.contents.len(), 2);

    // First message: original user request
    assert_eq!(gemini_req2.contents[0].role, Some("user".to_string()));

    // Second message: user message with TodoWrite tool result
    assert_eq!(gemini_req2.contents[1].role, Some("user".to_string()));

    // Verify that the second message contains the TodoWrite function response
    let has_todo_write_response = gemini_req2.contents[1].parts.iter().any(|p| {
        matches!(p, GeminiPart::FunctionResponse { function_response }
                if function_response.name == "TodoWrite")
    });
    assert!(
        has_todo_write_response,
        "TodoWrite results should be sent to Gemini"
    );
}

/// Test Bash tool (simple string parameter)
#[test]
fn test_e2e_bash_tool() {
    let mut properties = HashMap::new();
    properties.insert(
        "command".to_string(),
        Box::new(JsonSchema {
            schema_type: "string".to_string(),
            description: Some("Shell command to execute".to_string()),
            ..Default::default()
        }),
    );

    let bash_tool = ClaudeTool {
        name: "Bash".to_string(),
        description: "Execute bash commands".to_string(),
        input_schema: JsonSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["command".to_string()]),
            ..Default::default()
        },
    };

    let req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ContentType::Text("List files in current directory".to_string()),
        }],
        system: None,
        max_tokens: Some(1000),
        temperature: None,
        stop_sequences: None,
        stream: true,
        top_p: None,
        top_k: None,
        tools: Some(vec![bash_tool]),
    };

    let gemini_req = transform_request(req).unwrap();

    // Verify Bash tool transformed
    let tools = gemini_req.tools.unwrap();
    assert_eq!(tools[0].function_declarations[0].name, "Bash");

    // Verify required fields
    let params = &tools[0].function_declarations[0].parameters;
    assert_eq!(params.required, Some(vec!["command".to_string()]));
}

/// Test Edit tool (multiple parameters)
#[test]
fn test_e2e_edit_tool() {
    let mut properties = HashMap::new();
    properties.insert(
        "file_path".to_string(),
        Box::new(JsonSchema {
            schema_type: "string".to_string(),
            ..Default::default()
        }),
    );
    properties.insert(
        "old_string".to_string(),
        Box::new(JsonSchema {
            schema_type: "string".to_string(),
            ..Default::default()
        }),
    );
    properties.insert(
        "new_string".to_string(),
        Box::new(JsonSchema {
            schema_type: "string".to_string(),
            ..Default::default()
        }),
    );

    let edit_tool = ClaudeTool {
        name: "Edit".to_string(),
        description: "Edit file content".to_string(),
        input_schema: JsonSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec![
                "file_path".to_string(),
                "old_string".to_string(),
                "new_string".to_string(),
            ]),
            ..Default::default()
        },
    };

    let gemini_req = transform_request(ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ContentType::Text("Edit a file".to_string()),
        }],
        system: None,
        max_tokens: Some(1000),
        temperature: None,
        stop_sequences: None,
        stream: true,
        top_p: None,
        top_k: None,
        tools: Some(vec![edit_tool]),
    })
    .unwrap();

    let tools_result = gemini_req.tools.unwrap();
    let func = &tools_result[0].function_declarations[0];

    assert_eq!(func.name, "Edit");
    assert_eq!(func.parameters.required.as_ref().unwrap().len(), 3);
}

/// Test AskUserQuestion tool (complex nested structure)
#[test]
fn test_e2e_ask_user_question_complex() {
    // Option schema
    let mut option_props = HashMap::new();
    option_props.insert(
        "label".to_string(),
        Box::new(JsonSchema {
            schema_type: "string".to_string(),
            ..Default::default()
        }),
    );
    option_props.insert(
        "description".to_string(),
        Box::new(JsonSchema {
            schema_type: "string".to_string(),
            ..Default::default()
        }),
    );

    // Question schema
    let mut question_props = HashMap::new();
    question_props.insert(
        "question".to_string(),
        Box::new(JsonSchema {
            schema_type: "string".to_string(),
            ..Default::default()
        }),
    );
    question_props.insert(
        "header".to_string(),
        Box::new(JsonSchema {
            schema_type: "string".to_string(),
            ..Default::default()
        }),
    );
    question_props.insert(
        "options".to_string(),
        Box::new(JsonSchema {
            schema_type: "array".to_string(),
            items: Some(Box::new(JsonSchema {
                schema_type: "object".to_string(),
                properties: Some(option_props),
                required: Some(vec!["label".to_string(), "description".to_string()]),
                ..Default::default()
            })),
            ..Default::default()
        }),
    );
    question_props.insert(
        "multiSelect".to_string(),
        Box::new(JsonSchema {
            schema_type: "boolean".to_string(),
            ..Default::default()
        }),
    );

    // Top level
    let mut properties = HashMap::new();
    properties.insert(
        "questions".to_string(),
        Box::new(JsonSchema {
            schema_type: "array".to_string(),
            items: Some(Box::new(JsonSchema {
                schema_type: "object".to_string(),
                properties: Some(question_props),
                required: Some(vec![
                    "question".to_string(),
                    "header".to_string(),
                    "options".to_string(),
                    "multiSelect".to_string(),
                ]),
                ..Default::default()
            })),
            ..Default::default()
        }),
    );

    let ask_tool = ClaudeTool {
        name: "AskUserQuestion".to_string(),
        description: "Ask the user questions".to_string(),
        input_schema: JsonSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["questions".to_string()]),
            ..Default::default()
        },
    };

    // Validate the complex schema
    assert!(claude_code_proxy::validation::validate_tool_schema(&ask_tool).is_ok());

    // Transform
    let tools_result = transform_tools(vec![ask_tool]).unwrap();
    let func = &tools_result[0].function_declarations[0];

    assert_eq!(func.name, "AskUserQuestion");

    // Verify nested structure preserved
    let questions_schema = func
        .parameters
        .properties
        .as_ref()
        .unwrap()
        .get("questions")
        .unwrap();
    assert_eq!(questions_schema.schema_type, "array");

    let question_item = questions_schema.items.as_ref().unwrap();
    assert_eq!(question_item.schema_type, "object");

    let option_schema = question_item
        .properties
        .as_ref()
        .unwrap()
        .get("options")
        .unwrap();
    assert_eq!(option_schema.schema_type, "array");
}

/// Test error propagation through the entire flow
#[test]
fn test_e2e_error_handling() {
    let state = ConversationState::new();

    // Tool that will fail
    let mut properties = HashMap::new();
    properties.insert(
        "url".to_string(),
        Box::new(JsonSchema {
            schema_type: "string".to_string(),
            ..Default::default()
        }),
    );

    let web_tool = ClaudeTool {
        name: "WebFetch".to_string(),
        description: "Fetch web content".to_string(),
        input_schema: JsonSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["url".to_string()]),
            ..Default::default()
        },
    };

    // Initial request
    let req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ContentType::Text("Fetch https://example.com".to_string()),
        }],
        system: None,
        max_tokens: Some(1000),
        temperature: None,
        stop_sequences: None,
        stream: true,
        top_p: None,
        top_k: None,
        tools: Some(vec![web_tool]),
    };

    let gemini_req = transform_request_with_state(req, Some(&state), false).unwrap();
    assert!(gemini_req.tools.is_some());

    // Simulate function call
    let fc = FunctionCall {
        name: "WebFetch".to_string(),
        args: serde_json::json!({"url": "https://example.com"}),
    };

    let (tool_id, _) = transform_function_call(&fc).unwrap();
    state.register_tool_use(
        tool_id.clone(),
        "WebFetch".to_string(),
        None,
        serde_json::json!({}),
    );

    // Simulate error result
    let error_result = ContentBlock::ToolResult {
        tool_use_id: tool_id,
        content: "Connection timeout after 30s".to_string(),
        is_error: Some(true),
    };

    let function_response = transform_tool_result(&error_result, "WebFetch".to_string()).unwrap();

    match function_response {
        GeminiPart::FunctionResponse { function_response } => {
            assert_eq!(function_response.name, "WebFetch");
            assert_eq!(function_response.response["error"], true);
            assert!(
                function_response.response["result"]
                    .as_str()
                    .unwrap()
                    .contains("timeout")
            );
        }
        _ => panic!("Expected FunctionResponse"),
    }
}

/// Test all 11 Claude Code tools can be transformed
#[test]
fn test_all_claude_code_tools() {
    let tool_names = vec![
        "TodoWrite",
        "Task",
        "Bash",
        "Read",
        "Edit",
        "Write",
        "Glob",
        "Grep",
        "AskUserQuestion",
        "WebFetch",
        "WebSearch",
    ];

    for tool_name in tool_names {
        let tool = ClaudeTool {
            name: tool_name.to_string(),
            description: format!("Tool: {}", tool_name),
            input_schema: JsonSchema {
                schema_type: "object".to_string(),
                properties: Some(HashMap::new()),
                ..Default::default()
            },
        };

        let result = transform_tools(vec![tool]);
        assert!(result.is_ok(), "Failed to transform tool: {}", tool_name);

        let gemini_tools = result.unwrap();
        assert_eq!(gemini_tools[0].function_declarations[0].name, tool_name);
    }
}

/// Test cache effectiveness
/// Note: This test uses global TOOL_CACHE and may interfere with parallel tests
#[test]
#[ignore = "Uses global state - run with --ignored --test-threads=1"]
fn test_schema_caching() {
    use claude_code_proxy::cache::TOOL_CACHE;

    let initial_count = TOOL_CACHE.len();

    let tool = ClaudeTool {
        name: format!("cached_tool_{}", uuid::Uuid::new_v4().simple()),
        description: "Test caching".to_string(),
        input_schema: JsonSchema {
            schema_type: "object".to_string(),
            ..Default::default()
        },
    };

    // First transformation - cache miss
    let result1 = transform_tools(vec![tool.clone()]).unwrap();
    assert_eq!(TOOL_CACHE.len(), initial_count + 1);

    // Second transformation - cache hit
    let result2 = transform_tools(vec![tool.clone()]).unwrap();
    assert_eq!(TOOL_CACHE.len(), initial_count + 1); // Should not increase

    // Results should be identical
    assert_eq!(
        result1[0].function_declarations[0].name,
        result2[0].function_declarations[0].name
    );
}

/// Test metrics collection
/// Note: This test uses global TOOL_METRICS and may interfere with parallel tests
#[test]
#[ignore = "Uses global state - run with --ignored --test-threads=1"]
fn test_metrics_tracking() {
    use claude_code_proxy::metrics::TOOL_METRICS;

    TOOL_METRICS.reset();

    let tool = ClaudeTool {
        name: "metrics_test".to_string(),
        description: "Test metrics".to_string(),
        input_schema: JsonSchema {
            schema_type: "object".to_string(),
            ..Default::default()
        },
    };

    // Transform should record metrics
    transform_tools(vec![tool]).unwrap();

    let snapshot = TOOL_METRICS.snapshot();
    assert!(snapshot.total_calls > 0);
    assert!(snapshot.successful_transformations > 0);
}

/// Test that unsupported schema fields are filtered during serialization
#[test]
fn test_schema_field_filtering() {
    // Create a schema with fields that Gemini doesn't support
    let mut additional = HashMap::new();
    additional.insert("additionalProperties".to_string(), serde_json::json!(false));
    additional.insert(
        "$schema".to_string(),
        serde_json::json!("http://json-schema.org/draft-07/schema#"),
    );
    additional.insert("$id".to_string(), serde_json::json!("some-id"));

    let tool = ClaudeTool {
        name: "FilterTest".to_string(),
        description: "Test field filtering".to_string(),
        input_schema: JsonSchema {
            schema_type: "object".to_string(),
            additional,
            ..Default::default()
        },
    };

    let result = transform_tools(vec![tool]).unwrap();
    let json = serde_json::to_string_pretty(&result).unwrap();

    // Verify unsupported fields are NOT in the output
    assert!(
        !json.contains("additionalProperties"),
        "Should not serialize additionalProperties"
    );
    assert!(!json.contains("$schema"), "Should not serialize $schema");
    assert!(!json.contains("$id"), "Should not serialize $id");

    // Verify supported fields ARE present
    assert!(json.contains("FilterTest"));
    assert!(json.contains("object"));
}

/// Test validation catches errors early
#[test]
fn test_validation_prevents_invalid_tools() {
    use claude_code_proxy::validation::validate_tools;

    // Empty tool name
    let invalid_tool = ClaudeTool {
        name: "".to_string(),
        description: "Invalid".to_string(),
        input_schema: JsonSchema {
            schema_type: "object".to_string(),
            ..Default::default()
        },
    };

    assert!(validate_tools(&[invalid_tool]).is_err());

    // Duplicate names
    let tool1 = ClaudeTool {
        name: "duplicate".to_string(),
        description: "First".to_string(),
        input_schema: JsonSchema {
            schema_type: "object".to_string(),
            ..Default::default()
        },
    };

    let tool2 = ClaudeTool {
        name: "duplicate".to_string(),
        description: "Second".to_string(),
        input_schema: JsonSchema {
            schema_type: "object".to_string(),
            ..Default::default()
        },
    };

    assert!(validate_tools(&[tool1, tool2]).is_err());
}
