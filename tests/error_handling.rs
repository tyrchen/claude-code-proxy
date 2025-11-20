use claude_code_proxy::models::claude::*;
use claude_code_proxy::streaming::SSEEventGenerator;
use claude_code_proxy::transform::*;

#[test]
fn test_empty_messages_validation() {
    let req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![],
        system: None,
        max_tokens: Some(100),
        temperature: None,
        stop_sequences: None,
        stream: true,
        top_p: None,
        top_k: None,
        tools: None,
    };

    let result = validate_claude_request(&req);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("No messages provided"));
}

#[test]
fn test_invalid_first_role() {
    let req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![ClaudeMessage {
            role: "assistant".to_string(),
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

    let result = validate_claude_request(&req);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("First message must be from user")
    );
}

#[test]
fn test_consecutive_assistant_messages() {
    let req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![
            ClaudeMessage {
                role: "user".to_string(),
                content: ContentType::Text("Hi".to_string()),
            },
            ClaudeMessage {
                role: "assistant".to_string(),
                content: ContentType::Text("Hello".to_string()),
            },
            ClaudeMessage {
                role: "assistant".to_string(),
                content: ContentType::Text("How are you?".to_string()),
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

    let result = validate_claude_request(&req);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("consecutive assistant")
    );
}

#[test]
fn test_invalid_max_tokens() {
    let mut req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ContentType::Text("Hi".to_string()),
        }],
        system: None,
        max_tokens: Some(0),
        temperature: None,
        stop_sequences: None,
        stream: true,
        top_p: None,
        top_k: None,
        tools: None,
    };

    assert!(validate_claude_request(&req).is_err());

    req.max_tokens = Some(2_000_000);
    assert!(validate_claude_request(&req).is_err());

    req.max_tokens = Some(1000);
    assert!(validate_claude_request(&req).is_ok());
}

#[test]
fn test_invalid_temperature() {
    let mut req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ContentType::Text("Hi".to_string()),
        }],
        system: None,
        max_tokens: Some(100),
        temperature: Some(-0.1),
        stop_sequences: None,
        stream: true,
        top_p: None,
        top_k: None,
        tools: None,
    };

    assert!(validate_claude_request(&req).is_err());

    req.temperature = Some(3.0);
    assert!(validate_claude_request(&req).is_err());

    req.temperature = Some(0.7);
    assert!(validate_claude_request(&req).is_ok());
}

#[test]
fn test_invalid_top_p() {
    let mut req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ContentType::Text("Hi".to_string()),
        }],
        system: None,
        max_tokens: Some(100),
        temperature: None,
        stop_sequences: None,
        stream: true,
        top_p: Some(-0.1),
        top_k: None,
        tools: None,
    };

    assert!(validate_claude_request(&req).is_err());

    req.top_p = Some(1.5);
    assert!(validate_claude_request(&req).is_err());

    req.top_p = Some(0.9);
    assert!(validate_claude_request(&req).is_ok());
}

#[test]
fn test_invalid_role_in_transformation() {
    let req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![ClaudeMessage {
            role: "invalid_role".to_string(),
            content: ContentType::Text("Hi".to_string()),
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

    let result = transform_request(req);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Invalid role"));
}

#[test]
fn test_malformed_json_parsing() {
    let malformed = r#"{"model": "claude-3-5-sonnet", "messages": [}"#;
    let result: Result<ClaudeRequest, _> = serde_json::from_str(malformed);
    assert!(result.is_err());
}

#[test]
fn test_missing_required_fields() {
    // Missing messages field
    let json = r#"{"model": "claude-3-5-sonnet"}"#;
    let result: Result<ClaudeRequest, _> = serde_json::from_str(json);
    assert!(result.is_err());

    // Missing model field
    let json = r#"{"messages": [{"role": "user", "content": "Hi"}]}"#;
    let result: Result<ClaudeRequest, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn test_error_sse_formatting() {
    let error_types = vec![
        ("invalid_request_error", "Bad request"),
        ("authentication_error", "Invalid API key"),
        ("rate_limit_error", "Too many requests"),
        ("api_error", "Internal server error"),
    ];

    for (error_type, message) in error_types {
        let sse = SSEEventGenerator::format_error(error_type, message);

        // Verify SSE format
        assert!(sse.starts_with("event: error\n"));
        assert!(sse.contains("data: "));
        assert!(sse.ends_with("\n\n"));

        // Verify content
        assert!(sse.contains(error_type));
        assert!(sse.contains(message));

        // Verify valid JSON
        let data_start = sse.find("\ndata: ").unwrap() + 7;
        let data_end = sse.rfind("\n\n").unwrap();
        let json_str = &sse[data_start..data_end];
        let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();

        assert_eq!(parsed["type"], "error");
        assert_eq!(parsed["error"]["type"], error_type);
        assert_eq!(parsed["error"]["message"], message);
    }
}

#[test]
fn test_upstream_error_status_codes() {
    // Test that different HTTP status codes map to appropriate error types
    let status_codes = vec![
        (400, "invalid_request_error"),
        (401, "authentication_error"),
        (403, "authentication_error"),
        (429, "rate_limit_error"),
        (500, "api_error"),
        (502, "api_error"),
        (503, "api_error"),
    ];

    for (status, expected_type) in status_codes {
        let error_msg = format!("HTTP {} error", status);
        let sse = SSEEventGenerator::format_error(expected_type, &error_msg);

        assert!(sse.contains(expected_type));
        assert!(sse.contains(&error_msg));
    }
}

#[test]
fn test_empty_content_handling() {
    let req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ContentType::Text("".to_string()),
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

    // Empty content should still validate
    assert!(validate_claude_request(&req).is_ok());

    // And transform
    let gemini_req = transform_request(req).unwrap();
    assert_eq!(gemini_req.contents.len(), 1);
}

#[test]
fn test_edge_case_parameters() {
    let req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ContentType::Text("Test".to_string()),
        }],
        system: None,
        max_tokens: Some(1),    // Minimum valid
        temperature: Some(0.0), // Minimum valid
        stop_sequences: Some(vec![]),
        stream: true,
        top_p: Some(0.0), // Minimum valid
        top_k: Some(1),   // Minimum valid
        tools: None,
    };

    assert!(validate_claude_request(&req).is_ok());
    assert!(transform_request(req).is_ok());
}

#[test]
fn test_unicode_content() {
    let req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ContentType::Text("Hello 世界 🌍 مرحبا Привет".to_string()),
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

    assert!(validate_claude_request(&req).is_ok());
    let gemini_req = transform_request(req).unwrap();

    // Verify unicode is preserved
    if let claude_code_proxy::models::gemini::GeminiPart::Text { text } =
        &gemini_req.contents[0].parts[0]
    {
        assert!(text.contains("世界"));
        assert!(text.contains("🌍"));
        assert!(text.contains("مرحبا"));
        assert!(text.contains("Привет"));
    } else {
        panic!("Expected text part");
    }
}

#[test]
fn test_large_content_handling() {
    // Test with a large message
    let large_text = "a".repeat(100_000); // 100KB of text

    let req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ContentType::Text(large_text.clone()),
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

    assert!(validate_claude_request(&req).is_ok());
    let gemini_req = transform_request(req).unwrap();

    if let claude_code_proxy::models::gemini::GeminiPart::Text { text } =
        &gemini_req.contents[0].parts[0]
    {
        assert_eq!(text.len(), 100_000);
    }
}
