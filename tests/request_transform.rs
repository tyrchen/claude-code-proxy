use claude_code_proxy::models::claude::*;
use claude_code_proxy::transform::*;
use std::fs;

#[test]
fn test_transform_simple_request() {
    let json = fs::read_to_string("tests/fixtures/claude_request_simple.json").unwrap();
    let claude_req: ClaudeRequest = serde_json::from_str(&json).unwrap();

    // Validate
    assert!(validate_claude_request(&claude_req).is_ok());

    // Transform
    let gemini_req = transform_request(claude_req.clone()).unwrap();

    assert_eq!(gemini_req.contents.len(), 1);
    assert_eq!(gemini_req.contents[0].role, "user");
    assert!(gemini_req.system_instruction.is_none());

    // Verify generation config
    let gen_config = gemini_req.generation_config.unwrap();
    assert_eq!(gen_config.max_output_tokens, Some(100));

    // Verify model mapping
    let target_model = map_model_name(&claude_req.model);
    assert_eq!(target_model, "gemini-2.0-flash-exp");
}

#[test]
fn test_transform_request_with_system() {
    let json = fs::read_to_string("tests/fixtures/claude_request_with_system.json").unwrap();
    let claude_req: ClaudeRequest = serde_json::from_str(&json).unwrap();

    // Validate
    assert!(validate_claude_request(&claude_req).is_ok());

    // Transform
    let gemini_req = transform_request(claude_req).unwrap();

    assert!(gemini_req.system_instruction.is_some());
    let system_inst = gemini_req.system_instruction.unwrap();
    assert_eq!(system_inst.parts.len(), 1);

    // Verify generation config
    let gen_config = gemini_req.generation_config.unwrap();
    assert_eq!(gen_config.max_output_tokens, Some(500));
    assert_eq!(gen_config.temperature, Some(0.7));
}

#[test]
fn test_transform_request_with_blocks() {
    let json = fs::read_to_string("tests/fixtures/claude_request_blocks.json").unwrap();
    let claude_req: ClaudeRequest = serde_json::from_str(&json).unwrap();

    // Validate
    assert!(validate_claude_request(&claude_req).is_ok());

    // Transform
    let gemini_req = transform_request(claude_req.clone()).unwrap();

    assert_eq!(gemini_req.contents.len(), 1);
    assert_eq!(gemini_req.contents[0].parts.len(), 2); // Two text blocks

    // Verify model mapping for opus
    let target_model = map_model_name(&claude_req.model);
    assert_eq!(target_model, "gemini-1.5-pro"); // opus -> pro
}

#[test]
fn test_serialize_gemini_request() {
    let json = fs::read_to_string("tests/fixtures/claude_request_simple.json").unwrap();
    let claude_req: ClaudeRequest = serde_json::from_str(&json).unwrap();

    let gemini_req = transform_request(claude_req).unwrap();

    // Serialize to JSON
    let gemini_json = serde_json::to_string_pretty(&gemini_req).unwrap();

    // Verify camelCase formatting
    assert!(gemini_json.contains("maxOutputTokens"));
    assert!(gemini_json.contains("contents"));

    // Verify it can be deserialized back
    let _: claude_code_proxy::models::gemini::GeminiRequest =
        serde_json::from_str(&gemini_json).unwrap();
}

#[test]
fn test_validation_errors() {
    // Test empty messages
    let mut req = ClaudeRequest {
        model: "claude-3-5-sonnet".to_string(),
        messages: vec![],
        system: None,
        max_tokens: Some(100),
        temperature: None,
        stop_sequences: None,
        stream: true,
        top_p: None,
        top_k: None,
    };

    assert!(validate_claude_request(&req).is_err());

    // Test invalid temperature
    req.messages.push(ClaudeMessage {
        role: "user".to_string(),
        content: ContentType::Text("Hello".to_string()),
    });
    req.temperature = Some(3.0);

    assert!(validate_claude_request(&req).is_err());

    // Fix temperature
    req.temperature = Some(0.7);
    assert!(validate_claude_request(&req).is_ok());
}

#[test]
fn test_end_to_end_transformation() {
    // Load fixture
    let json = fs::read_to_string("tests/fixtures/claude_request_with_system.json").unwrap();
    let claude_req: ClaudeRequest = serde_json::from_str(&json).unwrap();

    // Validate
    validate_claude_request(&claude_req).expect("Validation failed");

    // Map model
    let target_model = map_model_name(&claude_req.model);

    // Transform
    let gemini_req = transform_request(claude_req).expect("Transformation failed");

    // Serialize
    let gemini_json = serde_json::to_string(&gemini_req).expect("Serialization failed");

    // Verify complete JSON structure
    assert!(gemini_json.contains("contents"));
    assert!(gemini_json.contains("systemInstruction"));
    assert!(gemini_json.contains("generationConfig"));

    println!("Target model: {}", target_model);
    println!(
        "Gemini request JSON:\n{}",
        serde_json::to_string_pretty(&gemini_req).unwrap()
    );
}
