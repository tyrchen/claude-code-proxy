use claude_code_proxy::streaming::{SSEEventGenerator, StreamingJsonParser};
use std::fs;

#[test]
fn test_parse_gemini_stream_fixture() {
    let json = fs::read_to_string("tests/fixtures/gemini_response_stream.json").unwrap();
    let mut parser = StreamingJsonParser::new();

    let chunks = parser.feed(json.as_bytes()).unwrap();

    assert_eq!(chunks.len(), 3);
    assert!(chunks[0].usage_metadata.is_some());
    assert!(chunks[2].candidates[0].finish_reason.is_some());
}

#[test]
fn test_complete_streaming_pipeline() {
    let mut parser = StreamingJsonParser::new();
    let mut generator = SSEEventGenerator::new("gemini-2.0-flash-exp".to_string());

    // Simulate streaming chunks from Gemini
    let chunk1 = br#"[{"candidates":[{"content":{"parts":[{"text":"Hello"}],"role":"model"}}],"usageMetadata":{"promptTokenCount":10}}]"#;
    let chunk2 = br#",{"candidates":[{"content":{"parts":[{"text":" world"}],"role":"model"}}]}]"#;
    let chunk3 = br#",{"candidates":[{"content":{"parts":[{"text":"!"}],"role":"model"},"finishReason":"STOP"}],"usageMetadata":{"candidatesTokenCount":5}}]"#;

    let mut all_events = Vec::new();

    // Process first chunk
    let parsed1 = parser.feed(chunk1).unwrap();
    for gemini_chunk in parsed1 {
        let events = generator.generate_events(gemini_chunk);
        all_events.extend(events);
    }

    // Process second chunk
    let parsed2 = parser.feed(chunk2).unwrap();
    for gemini_chunk in parsed2 {
        let events = generator.generate_events(gemini_chunk);
        all_events.extend(events);
    }

    // Process third chunk
    let parsed3 = parser.feed(chunk3).unwrap();
    for gemini_chunk in parsed3 {
        let events = generator.generate_events(gemini_chunk);
        all_events.extend(events);
    }

    // Verify event sequence
    assert!(all_events.iter().any(|e| e.contains("message_start")));
    assert!(all_events.iter().any(|e| e.contains("content_block_start")));
    assert!(all_events.iter().any(|e| e.contains("content_block_delta")));
    assert!(all_events.iter().any(|e| e.contains("content_block_stop")));
    assert!(all_events.iter().any(|e| e.contains("message_delta")));
    assert!(all_events.iter().any(|e| e.contains("message_stop")));

    // Verify text content
    let combined: String = all_events.join("");
    assert!(combined.contains("Hello"));
    assert!(combined.contains(" world"));
    assert!(combined.contains("!"));

    println!("Generated {} SSE events", all_events.len());
    println!("First event:\n{}", all_events[0]);
}

#[test]
fn test_incremental_parsing() {
    let mut parser = StreamingJsonParser::new();

    // Split a JSON object across multiple network packets
    let part1 = b"[{\"candidates\":[{\"content\":{";
    let part2 = b"\"parts\":[{\"text\":\"streaming\"}]";
    let part3 = b",\"role\":\"model\"}}]}]";

    // First two parts should not yield complete objects
    assert_eq!(parser.feed(part1).unwrap().len(), 0);
    assert_eq!(parser.feed(part2).unwrap().len(), 0);

    // Third part completes the object
    let results = parser.feed(part3).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].candidates[0].content.as_ref().unwrap().parts[0].clone(),
        claude_code_proxy::models::gemini::GeminiPart::Text {
            text: "streaming".to_string()
        }
    );
}

#[test]
fn test_sse_format_validity() {
    let mut generator = SSEEventGenerator::new("gemini-test".to_string());

    let chunk = serde_json::from_str::<claude_code_proxy::models::gemini::GeminiStreamChunk>(
        r#"{"candidates":[{"content":{"parts":[{"text":"test"}],"role":"model"}}]}"#,
    )
    .unwrap();

    let events = generator.generate_events(chunk);

    // Verify SSE format
    for event in &events {
        // Each event should start with "event: "
        assert!(event.starts_with("event: "));

        // Each event should contain "data: "
        assert!(event.contains("\ndata: "));

        // Each event should end with double newline
        assert!(event.ends_with("\n\n"));

        // Data should be valid JSON
        let data_start = event.find("\ndata: ").unwrap() + 7;
        let data_end = event.rfind("\n\n").unwrap();
        let json_str = &event[data_start..data_end];
        assert!(serde_json::from_str::<serde_json::Value>(json_str).is_ok());
    }
}

#[test]
fn test_error_event_format() {
    let error_sse = SSEEventGenerator::format_error("authentication_error", "Invalid API key");

    assert!(error_sse.contains("event: error"));
    assert!(error_sse.contains("authentication_error"));
    assert!(error_sse.contains("Invalid API key"));
    assert!(error_sse.ends_with("\n\n"));

    // Verify it's valid SSE + JSON
    let data_start = error_sse.find("\ndata: ").unwrap() + 7;
    let data_end = error_sse.rfind("\n\n").unwrap();
    let json_str = &error_sse[data_start..data_end];
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();

    assert_eq!(parsed["type"], "error");
    assert_eq!(parsed["error"]["type"], "authentication_error");
}

#[test]
fn test_realistic_streaming_scenario() {
    // Simulate a realistic Gemini API response arriving in chunks
    let mut parser = StreamingJsonParser::new();
    let mut generator = SSEEventGenerator::new("gemini-1.5-pro".to_string());

    let fixture = fs::read_to_string("tests/fixtures/gemini_response_stream.json").unwrap();

    // Split the fixture into arbitrary chunks to simulate network
    let bytes = fixture.as_bytes();
    let chunk_size = 50;
    let mut all_events = Vec::new();

    for chunk in bytes.chunks(chunk_size) {
        let parsed = parser.feed(chunk).unwrap();
        for gemini_chunk in parsed {
            let events = generator.generate_events(gemini_chunk);
            all_events.extend(events);
        }
    }

    // Verify we got a complete SSE stream
    assert!(!all_events.is_empty());
    assert!(all_events.iter().any(|e| e.contains("message_start")));
    assert!(all_events.iter().any(|e| e.contains("message_stop")));

    // Verify token counts
    let (input_tokens, output_tokens) = generator.token_counts();
    assert!(input_tokens > 0);
    assert!(output_tokens > 0);

    println!("Processed stream:");
    println!("  Input tokens: {}", input_tokens);
    println!("  Output tokens: {}", output_tokens);
    println!("  Total events: {}", all_events.len());
}

#[test]
fn test_parser_handles_malformed_gracefully() {
    let mut parser = StreamingJsonParser::new();

    // Invalid JSON in the middle
    let data = b"[{\"candidates\":[]},{\"invalid json\",{\"candidates\":[]}]";

    // Should continue processing despite error
    let results = parser.feed(data).unwrap();

    // Should get the first valid object, skip invalid, get last valid
    assert!(results.len() >= 1);
}

#[test]
fn test_finish_reason_variations() {
    let _generator = SSEEventGenerator::new("test".to_string());

    let test_cases = vec![
        ("STOP", "end_turn"),
        ("MAX_TOKENS", "max_tokens"),
        ("SAFETY", "stop_sequence"),
        ("RECITATION", "stop_sequence"),
    ];

    for (gemini_reason, expected_claude_reason) in test_cases {
        let chunk =
            serde_json::from_str::<claude_code_proxy::models::gemini::GeminiStreamChunk>(&format!(
                r#"{{"candidates":[{{"finishReason":"{}"}}]}}"#,
                gemini_reason
            ))
            .unwrap();

        // Reset generator for each test
        let mut gen = SSEEventGenerator::new("test".to_string());
        gen.generate_events(
            serde_json::from_str(
                r#"{"candidates":[{"content":{"parts":[{"text":"x"}],"role":"model"}}]}"#,
            )
            .unwrap(),
        ); // Send header

        let events = gen.generate_events(chunk);
        let combined = events.join("");

        assert!(
            combined.contains(expected_claude_reason),
            "Expected {} for finish reason {}",
            expected_claude_reason,
            gemini_reason
        );
    }
}
