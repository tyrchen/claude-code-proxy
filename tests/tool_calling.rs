use claude_code_proxy::models::claude::*;
use claude_code_proxy::models::gemini::*;
use claude_code_proxy::state::ConversationState;
use claude_code_proxy::transform::*;
use std::collections::HashMap;

/// Test end-to-end tool transformation: Claude request with tools -> Gemini request
#[test]
fn test_transform_request_with_tools() {
    let mut properties = HashMap::new();
    properties.insert(
        "location".to_string(),
        Box::new(JsonSchema {
            schema_type: "string".to_string(),
            description: Some("City name".to_string()),
            ..Default::default()
        }),
    );

    let claude_req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ContentType::Text("What's the weather in SF?".to_string()),
        }],
        system: None,
        max_tokens: Some(1000),
        temperature: None,
        stop_sequences: None,
        stream: true,
        top_p: None,
        top_k: None,
        tools: Some(vec![ClaudeTool {
            name: "get_weather".to_string(),
            description: "Get current weather for a location".to_string(),
            input_schema: JsonSchema {
                schema_type: "object".to_string(),
                properties: Some(properties),
                required: Some(vec!["location".to_string()]),
                ..Default::default()
            },
        }]),
    };

    let gemini_req = transform_request(claude_req).unwrap();

    // Verify tools were transformed
    assert!(gemini_req.tools.is_some());
    let tools = gemini_req.tools.unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].function_declarations.len(), 1);

    let func = &tools[0].function_declarations[0];
    assert_eq!(func.name, "get_weather");
    assert_eq!(func.description, "Get current weather for a location");
    assert_eq!(func.parameters.schema_type, "object");
    assert!(func.parameters.properties.is_some());
}

/// Test function call to tool_use transformation
#[test]
fn test_function_call_to_tool_use() {
    let function_call = FunctionCall {
        name: "get_weather".to_string(),
        args: serde_json::json!({
            "location": "San Francisco"
        }),
    };

    let (tool_use_id, content_block) = transform_function_call(&function_call).unwrap();

    // Verify ID format
    assert!(tool_use_id.starts_with("toolu_"));
    assert_eq!(tool_use_id.len(), 38); // toolu_ + 32 char UUID

    // Verify content block
    match content_block {
        ContentBlock::ToolUse { id, name, input } => {
            assert_eq!(id, tool_use_id);
            assert_eq!(name, "get_weather");
            assert_eq!(input["location"], "San Francisco");
        }
        _ => panic!("Expected ToolUse block"),
    }
}

/// Test tool result to function response with state lookup
#[test]
fn test_tool_result_with_state() {
    let state = ConversationState::new();

    // Register a tool use
    state.register_tool_use(
        "toolu_abc123".to_string(),
        "get_weather".to_string(),
        None,
        serde_json::json!({}),
    );

    // Create tool result
    let tool_result = ContentBlock::ToolResult {
        tool_use_id: "toolu_abc123".to_string(),
        content: "Sunny, 72°F".to_string(),
        is_error: None,
    };

    // Transform with state
    let gemini_part = transform_tool_result(&tool_result, "get_weather".to_string()).unwrap();

    match gemini_part {
        GeminiPart::FunctionResponse { function_response } => {
            assert_eq!(function_response.name, "get_weather");
            assert_eq!(function_response.response["result"], "Sunny, 72°F");
            assert_eq!(function_response.response["error"], false);
        }
        _ => panic!("Expected FunctionResponse"),
    }
}

/// Test multi-turn conversation with tool calling
#[test]
fn test_multi_turn_tool_conversation() {
    let state = ConversationState::new();

    // Turn 1: Initial request with tool definitions
    let mut properties = HashMap::new();
    properties.insert(
        "query".to_string(),
        Box::new(JsonSchema {
            schema_type: "string".to_string(),
            ..Default::default()
        }),
    );

    let turn1_req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ContentType::Text("Search for Rust tutorials".to_string()),
        }],
        system: None,
        max_tokens: Some(1000),
        temperature: None,
        stop_sequences: None,
        stream: true,
        top_p: None,
        top_k: None,
        tools: Some(vec![ClaudeTool {
            name: "web_search".to_string(),
            description: "Search the web".to_string(),
            input_schema: JsonSchema {
                schema_type: "object".to_string(),
                properties: Some(properties),
                required: Some(vec!["query".to_string()]),
                ..Default::default()
            },
        }]),
    };

    let gemini_req1 = transform_request_with_state(turn1_req, Some(&state), false).unwrap();
    assert!(gemini_req1.tools.is_some());

    // Simulate Gemini response with function call (would come from SSEEventGenerator)
    state.register_tool_use(
        "toolu_search_123".to_string(),
        "web_search".to_string(),
        None,
        serde_json::json!({"query": "test"}),
    );

    // Turn 2: Send tool result back
    let turn2_req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![
            ClaudeMessage {
                role: "user".to_string(),
                content: ContentType::Text("Search for Rust tutorials".to_string()),
            },
            ClaudeMessage {
                role: "assistant".to_string(),
                content: ContentType::Blocks(vec![ContentBlock::ToolUse {
                    id: "toolu_search_123".to_string(),
                    name: "web_search".to_string(),
                    input: serde_json::json!({"query": "Rust tutorials"}),
                }]),
            },
            ClaudeMessage {
                role: "user".to_string(),
                content: ContentType::Blocks(vec![ContentBlock::ToolResult {
                    tool_use_id: "toolu_search_123".to_string(),
                    content: "Found 10 great Rust tutorials".to_string(),
                    is_error: None,
                }]),
            },
        ],
        system: None,
        max_tokens: Some(1000),
        temperature: None,
        stop_sequences: None,
        stream: true,
        top_p: None,
        top_k: None,
        tools: None, // Tools not needed for follow-up
    };

    let gemini_req2 = transform_request_with_state(turn2_req, Some(&state), false).unwrap();

    // Verify function response was created with correct name from state
    let last_msg = gemini_req2.contents.last().unwrap();
    assert!(
        last_msg
            .parts
            .iter()
            .any(|p| matches!(p, GeminiPart::FunctionResponse { .. }))
    );
}

/// Test parallel tool calls (Claude supports multiple tools in one response)
#[test]
fn test_multiple_function_calls() {
    let fc1 = FunctionCall {
        name: "get_weather".to_string(),
        args: serde_json::json!({"location": "SF"}),
    };

    let fc2 = FunctionCall {
        name: "get_time".to_string(),
        args: serde_json::json!({"timezone": "PST"}),
    };

    let (id1, block1) = transform_function_call(&fc1).unwrap();
    let (id2, block2) = transform_function_call(&fc2).unwrap();

    // IDs should be unique
    assert_ne!(id1, id2);

    // Both should be valid tool_use blocks
    assert!(matches!(block1, ContentBlock::ToolUse { .. }));
    assert!(matches!(block2, ContentBlock::ToolUse { .. }));
}

/// Test complex nested schema transformation
#[test]
fn test_complex_tool_schema() {
    let mut coord_props = HashMap::new();
    coord_props.insert(
        "lat".to_string(),
        Box::new(JsonSchema {
            schema_type: "number".to_string(),
            minimum: Some(-90.0),
            maximum: Some(90.0),
            ..Default::default()
        }),
    );
    coord_props.insert(
        "lon".to_string(),
        Box::new(JsonSchema {
            schema_type: "number".to_string(),
            minimum: Some(-180.0),
            maximum: Some(180.0),
            ..Default::default()
        }),
    );

    let mut properties = HashMap::new();
    properties.insert(
        "coordinates".to_string(),
        Box::new(JsonSchema {
            schema_type: "object".to_string(),
            properties: Some(coord_props),
            required: Some(vec!["lat".to_string(), "lon".to_string()]),
            ..Default::default()
        }),
    );
    properties.insert(
        "radius".to_string(),
        Box::new(JsonSchema {
            schema_type: "number".to_string(),
            minimum: Some(0.0),
            description: Some("Search radius in kilometers".to_string()),
            ..Default::default()
        }),
    );

    let claude_tool = ClaudeTool {
        name: "search_nearby".to_string(),
        description: "Search for places nearby".to_string(),
        input_schema: JsonSchema {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: Some(vec!["coordinates".to_string()]),
            ..Default::default()
        },
    };

    let gemini_tools = transform_tools(vec![claude_tool]).unwrap();
    let func = &gemini_tools[0].function_declarations[0];

    // Verify nested structure preserved
    assert_eq!(func.name, "search_nearby");
    assert!(func.parameters.properties.is_some());

    let props = func.parameters.properties.as_ref().unwrap();
    let coords = props.get("coordinates").unwrap();
    assert_eq!(coords.schema_type, "object");
    assert!(coords.properties.is_some());

    let coord_props = coords.properties.as_ref().unwrap();
    assert!(coord_props.contains_key("lat"));
    assert!(coord_props.contains_key("lon"));

    // Verify constraints preserved
    let lat = coord_props.get("lat").unwrap();
    assert_eq!(lat.minimum, Some(-90.0));
    assert_eq!(lat.maximum, Some(90.0));
}

/// Test TodoWrite tool (the actual tool that was failing)
#[test]
fn test_todo_write_tool_transformation() {
    let mut todo_props = HashMap::new();
    todo_props.insert(
        "content".to_string(),
        Box::new(JsonSchema {
            schema_type: "string".to_string(),
            ..Default::default()
        }),
    );
    todo_props.insert(
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
    todo_props.insert(
        "activeForm".to_string(),
        Box::new(JsonSchema {
            schema_type: "string".to_string(),
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
                properties: Some(todo_props),
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

    let gemini_tools = transform_tools(vec![todo_tool]).unwrap();
    let func = &gemini_tools[0].function_declarations[0];

    assert_eq!(func.name, "TodoWrite");

    // Verify array schema
    let todos_prop = func
        .parameters
        .properties
        .as_ref()
        .unwrap()
        .get("todos")
        .unwrap();
    assert_eq!(todos_prop.schema_type, "array");
    assert!(todos_prop.items.is_some());

    // Verify enum for status
    let item_schema = todos_prop.items.as_ref().unwrap();
    let item_props = item_schema.properties.as_ref().unwrap();
    let status_prop = item_props.get("status").unwrap();
    assert!(status_prop.enum_values.is_some());
    assert_eq!(status_prop.enum_values.as_ref().unwrap().len(), 3);
}

/// Test state-based tool result lookup
#[test]
fn test_tool_result_state_lookup() {
    let state = ConversationState::new();

    // Register multiple tool uses
    state.register_tool_use(
        "toolu_001".to_string(),
        "tool_a".to_string(),
        None,
        serde_json::json!({}),
    );
    state.register_tool_use(
        "toolu_002".to_string(),
        "tool_b".to_string(),
        None,
        serde_json::json!({}),
    );
    state.register_tool_use(
        "toolu_003".to_string(),
        "tool_c".to_string(),
        None,
        serde_json::json!({}),
    );

    // Create request with tool results
    let claude_req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ContentType::Blocks(vec![
                ContentBlock::ToolResult {
                    tool_use_id: "toolu_001".to_string(),
                    content: "Result A".to_string(),
                    is_error: None,
                },
                ContentBlock::ToolResult {
                    tool_use_id: "toolu_002".to_string(),
                    content: "Result B".to_string(),
                    is_error: None,
                },
            ]),
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

    let gemini_req = transform_request_with_state(claude_req, Some(&state), false).unwrap();

    // Verify function responses
    let parts = &gemini_req.contents[0].parts;
    assert_eq!(parts.len(), 2);

    // Check both parts are function responses with correct names
    let mut found_names = Vec::new();
    for part in parts {
        match part {
            GeminiPart::FunctionResponse { function_response } => {
                found_names.push(function_response.name.clone());
            }
            _ => panic!("Expected FunctionResponse"),
        }
    }

    assert_eq!(found_names, vec!["tool_a", "tool_b"]);
}

/// Test error handling in tool results
#[test]
fn test_tool_result_with_error() {
    let state = ConversationState::new();
    state.register_tool_use(
        "toolu_err".to_string(),
        "failing_tool".to_string(),
        None,
        serde_json::json!({}),
    );

    let tool_result = ContentBlock::ToolResult {
        tool_use_id: "toolu_err".to_string(),
        content: "API rate limit exceeded".to_string(),
        is_error: Some(true),
    };

    let gemini_part = transform_tool_result(&tool_result, "failing_tool".to_string()).unwrap();

    match gemini_part {
        GeminiPart::FunctionResponse { function_response } => {
            assert_eq!(function_response.name, "failing_tool");
            assert_eq!(function_response.response["error"], true);
            assert!(
                function_response.response["result"]
                    .as_str()
                    .unwrap()
                    .contains("rate limit")
            );
        }
        _ => panic!("Expected FunctionResponse"),
    }
}

/// Test serialization of complete request with tools
#[test]
fn test_serialize_complete_request_with_tools() {
    let mut properties = HashMap::new();
    properties.insert(
        "text".to_string(),
        Box::new(JsonSchema {
            schema_type: "string".to_string(),
            ..Default::default()
        }),
    );

    let claude_req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ContentType::Text("Echo hello".to_string()),
        }],
        system: Some(SystemPrompt::Text("You are helpful".to_string())),
        max_tokens: Some(500),
        temperature: Some(0.7),
        stop_sequences: None,
        stream: true,
        top_p: None,
        top_k: None,
        tools: Some(vec![ClaudeTool {
            name: "echo".to_string(),
            description: "Echo text back".to_string(),
            input_schema: JsonSchema {
                schema_type: "object".to_string(),
                properties: Some(properties),
                required: Some(vec!["text".to_string()]),
                ..Default::default()
            },
        }]),
    };

    let gemini_req = transform_request(claude_req).unwrap();

    // Serialize to JSON
    let json = serde_json::to_string_pretty(&gemini_req).unwrap();

    // Debug: print JSON to see actual format
    println!("Serialized JSON:\n{}", json);

    // Verify structure (use loose matching since serde may format differently)
    assert!(json.contains("functionDeclarations"));
    assert!(json.contains("echo")); // Name will be there, just maybe formatted differently
    assert!(json.contains("systemInstruction"));
    assert!(json.contains("generationConfig"));

    // Verify it can be deserialized back
    let _parsed: GeminiRequest = serde_json::from_str(&json).unwrap();
}
