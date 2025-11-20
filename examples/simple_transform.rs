use claude_code_proxy::models::claude::*;
use claude_code_proxy::transform::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a simple Claude request
    let claude_request = ClaudeRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: ContentType::Text("What is Rust?".to_string()),
        }],
        system: Some(SystemPrompt::Text(
            "You are a professional Rust engineer.".to_string(),
        )),
        max_tokens: Some(500),
        temperature: Some(0.7),
        stop_sequences: None,
        stream: true,
        top_p: None,
        top_k: None,
        tools: None,
    };

    println!("Original Claude Request:");
    println!("{}", serde_json::to_string_pretty(&claude_request)?);
    println!("\n{}", "=".repeat(80));

    // Validate the request
    validate_claude_request(&claude_request)?;
    println!("\n✅ Request validation passed");

    // Map the model name
    let target_model = map_model_name(&claude_request.model);
    println!(
        "✅ Model mapping: {} -> {}",
        claude_request.model, target_model
    );

    // Transform to Gemini format
    let gemini_request = transform_request(claude_request)?;
    println!("\n{}", "=".repeat(80));
    println!("\nTransformed Gemini Request:");
    println!("{}", serde_json::to_string_pretty(&gemini_request)?);

    // Serialize for API call
    let json_bytes = serde_json::to_vec(&gemini_request)?;
    println!("\n{}", "=".repeat(80));
    println!("\n✅ Serialized size: {} bytes", json_bytes.len());
    println!("✅ Transformation complete!");

    Ok(())
}
