use claude_code_proxy::streaming::{SSEEventGenerator, StreamingJsonParser};
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Streaming Response Transformation Demo\n");
    println!("{}", "=".repeat(80));

    // Load a sample Gemini response
    let gemini_response = fs::read_to_string("tests/fixtures/gemini_response_stream.json")?;
    println!("\nOriginal Gemini Response:");
    println!("{}", gemini_response);

    println!("\n{}", "=".repeat(80));
    println!("\nParsing and Transforming to SSE...\n");

    // Initialize parser and generator
    let mut parser = StreamingJsonParser::new();
    let mut generator = SSEEventGenerator::new("gemini-3-pro-preview".to_string());

    // Simulate chunked arrival by splitting the response
    let bytes = gemini_response.as_bytes();
    let chunk_size = 100; // Simulate small network chunks

    let mut event_count = 0;
    let mut total_text = String::new();

    for (i, chunk) in bytes.chunks(chunk_size).enumerate() {
        println!("📦 Chunk {} ({} bytes)", i + 1, chunk.len());

        // Parse chunk
        let parsed_chunks = parser.feed(chunk)?;

        if parsed_chunks.is_empty() {
            println!("   ⏳ Incomplete - buffering...");
            continue;
        }

        println!("   ✅ Parsed {} JSON object(s)", parsed_chunks.len());

        // Generate SSE events
        for gemini_chunk in parsed_chunks {
            let events = generator.generate_events(gemini_chunk);

            for event in events {
                event_count += 1;

                // Extract text from delta events
                if event.contains("content_block_delta")
                    && let Some(text_start) = event.find(r#""text":""#)
                    && let Some(text_end) = event[text_start + 8..].find('"')
                {
                    let text = &event[text_start + 8..text_start + 8 + text_end];
                    total_text.push_str(text);
                }

                // Print SSE event (truncated)
                let preview = if event.len() > 100 {
                    format!("{}...", &event[..100])
                } else {
                    event.clone()
                };
                println!("   📤 SSE Event: {}", preview.replace('\n', "\\n"));
            }
        }
    }

    println!("\n{}", "=".repeat(80));
    println!("\n📊 Statistics:");
    println!("  Total SSE events: {}", event_count);
    println!("  Generated text: \"{}\"", total_text);

    let (input_tokens, output_tokens) = generator.token_counts();
    println!("  Input tokens: {}", input_tokens);
    println!("  Output tokens: {}", output_tokens);

    println!("\n✅ Streaming transformation complete!");

    Ok(())
}
