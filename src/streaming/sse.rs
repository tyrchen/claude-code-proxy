use crate::models::gemini::{GeminiPart, GeminiStreamChunk};
use crate::state::ConversationState;

/// Converts Gemini chunks to Claude SSE events
pub struct SSEEventGenerator {
    header_sent: bool,
    input_tokens: u32,
    output_tokens: u32,
    model_name: String,
    state: ConversationState,
    content_block_index: u32,
}

impl SSEEventGenerator {
    pub fn new(model_name: String) -> Self {
        Self {
            header_sent: false,
            input_tokens: 0,
            output_tokens: 0,
            model_name,
            state: ConversationState::new(),
            content_block_index: 0,
        }
    }

    pub fn with_state(model_name: String, state: ConversationState) -> Self {
        Self {
            header_sent: false,
            input_tokens: 0,
            output_tokens: 0,
            model_name,
            state,
            content_block_index: 0,
        }
    }

    pub fn generate_events(&mut self, chunk: GeminiStreamChunk) -> Vec<String> {
        let mut events = Vec::new();

        // Update token counts from usage metadata (Gemini provides actual counts)
        if let Some(usage) = &chunk.usage_metadata {
            if let Some(prompt_tokens) = usage.prompt_token_count {
                self.input_tokens = prompt_tokens;
            }
            if let Some(output) = usage.candidates_token_count {
                self.output_tokens = output;
            }
        }

        // Send header events on first chunk ONLY if we're going to have content
        // Skip headers if this is an empty response after tool use
        if !self.header_sent && self.chunk_has_meaningful_content(&chunk) {
            events.push(self.format_message_start());
            events.push(self.format_content_block_start());
            self.header_sent = true;
        }

        // Process candidates
        if let Some(candidate) = chunk.candidates.first() {
            if let Some(content) = &candidate.content {
                for part in &content.parts {
                    match part {
                        GeminiPart::Text { text } => {
                            // Skip empty or whitespace-only text
                            if !text.trim().is_empty() {
                                // Send headers if not sent yet (for non-empty text)
                                if !self.header_sent {
                                    events.push(self.format_message_start());
                                    events.push(self.format_content_block_start());
                                    self.header_sent = true;
                                }
                                events.push(self.format_content_block_delta(text));
                            }
                        }
                        GeminiPart::TextWithThought { text, .. } => {
                            if !text.trim().is_empty() {
                                if !self.header_sent {
                                    events.push(self.format_message_start());
                                    events.push(self.format_content_block_start());
                                    self.header_sent = true;
                                }
                                events.push(self.format_content_block_delta(text));
                            }
                        }
                        GeminiPart::FunctionCallWithThought {
                            function_call,
                            thought_signature,
                        } => {
                            tracing::info!(
                                tool_name = %function_call.name,
                                has_args = !function_call.args.is_null(),
                                args = ?function_call.args,
                                has_signature = !thought_signature.is_empty(),
                                "Gemini called tool WITH thought_signature"
                            );

                            // Transform to tool_use block
                            match crate::transform::tools::transform_function_call(function_call) {
                                Ok((tool_use_id, content_block)) => {
                                    // Register the mapping INCLUDING thought signature AND args
                                    self.state.register_tool_use(
                                        tool_use_id.clone(),
                                        function_call.name.clone(),
                                        Some(thought_signature.clone()),
                                        function_call.args.clone(),
                                    );

                                    // Emit tool_use content block (start + delta)
                                    events.push(self.format_tool_use_start(&content_block));
                                    // Emit content_block_stop for this tool
                                    events.push(self.format_tool_use_stop());
                                }
                                Err(e) => {
                                    tracing::error!(
                                        error = %e,
                                        "Failed to transform function call"
                                    );
                                }
                            }
                        }
                        GeminiPart::FunctionCall { function_call } => {
                            tracing::info!(
                                tool_name = %function_call.name,
                                has_args = !function_call.args.is_null(),
                                args = ?function_call.args,
                                "Gemini called tool WITHOUT thought_signature (parallel call)"
                            );

                            // Transform to tool_use block (no thought signature - parallel call)
                            match crate::transform::tools::transform_function_call(function_call) {
                                Ok((tool_use_id, content_block)) => {
                                    // Register the mapping WITHOUT thought signature BUT with args
                                    self.state.register_tool_use(
                                        tool_use_id.clone(),
                                        function_call.name.clone(),
                                        None,
                                        function_call.args.clone(),
                                    );

                                    // Emit tool_use content block (start + delta)
                                    events.push(self.format_tool_use_start(&content_block));
                                    // Emit content_block_stop for this tool
                                    events.push(self.format_tool_use_stop());
                                }
                                Err(e) => {
                                    tracing::error!(
                                        error = %e,
                                        "Failed to transform function call"
                                    );
                                }
                            }
                        }
                        GeminiPart::InlineData { .. } => {
                            // Skip inline data in responses for now
                        }
                        GeminiPart::FunctionResponse { .. } => {
                            // This shouldn't appear in responses from Gemini
                            tracing::warn!("Unexpected function response in Gemini output");
                        }
                    }
                }
            }

            // Handle finish
            if let Some(finish_reason) = &candidate.finish_reason {
                // Use context-aware stop reason determination
                let stop_reason = self.determine_stop_reason_with_context(&chunk, finish_reason);

                // Send headers if we haven't sent them yet and this is a finish
                // This handles the case where Gemini returns only whitespace after tool use
                if !self.header_sent {
                    tracing::debug!("Sending headers for empty response");
                    events.push(self.format_message_start());
                    events.push(self.format_content_block_start());
                    self.header_sent = true;
                }

                // Only send content_block_stop if we haven't already sent individual stops for tool_use blocks
                // For tool calls, we already sent stops for each tool above
                if !self.has_function_call_in_chunk(&chunk) {
                    events.push(self.format_content_block_stop());
                }

                events.push(self.format_message_delta(stop_reason));
                events.push(self.format_message_stop());
            }
        }

        events
    }

    fn format_message_start(&self) -> String {
        let data = serde_json::json!({
            "type": "message_start",
            "message": {
                "id": format!("msg_gemini_{}", uuid::Uuid::new_v4()),
                "type": "message",
                "role": "assistant",
                "model": self.model_name,
                "content": [],
                "stop_reason": null,
                "stop_sequence": null,
                "usage": {
                    "input_tokens": self.input_tokens,
                    "output_tokens": 1
                }
            }
        });
        format!("event: message_start\ndata: {}\n\n", data)
    }

    fn format_content_block_start(&self) -> String {
        let data = serde_json::json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": {
                "type": "text",
                "text": ""
            }
        });
        format!("event: content_block_start\ndata: {}\n\n", data)
    }

    fn format_content_block_delta(&self, text: &str) -> String {
        let data = serde_json::json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {
                "type": "text_delta",
                "text": text
            }
        });
        format!("event: content_block_delta\ndata: {}\n\n", data)
    }

    fn format_content_block_stop(&self) -> String {
        let data = serde_json::json!({
            "type": "content_block_stop",
            "index": 0
        });
        format!("event: content_block_stop\ndata: {}\n\n", data)
    }

    fn format_tool_use_stop(&self) -> String {
        // Use the last assigned index (content_block_index - 1)
        let index = self.content_block_index.saturating_sub(1);
        let data = serde_json::json!({
            "type": "content_block_stop",
            "index": index
        });
        format!("event: content_block_stop\ndata: {}\n\n", data)
    }

    fn format_message_delta(&self, stop_reason: &str) -> String {
        let data = serde_json::json!({
            "type": "message_delta",
            "delta": {
                "stop_reason": stop_reason,
                "stop_sequence": null
            },
            "usage": {
                "output_tokens": self.output_tokens
            }
        });
        format!("event: message_delta\ndata: {}\n\n", data)
    }

    fn format_message_stop(&self) -> String {
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".to_string()
    }

    /// Map Gemini finish reason to Claude stop reason with context awareness
    ///
    /// Enhanced to handle tool use and other edge cases intelligently.
    fn map_finish_reason(&self, gemini_reason: &str) -> &'static str {
        match gemini_reason {
            "STOP" => "end_turn",
            "MAX_TOKENS" => "max_tokens",
            "SAFETY" => "stop_sequence", // Content filtered by safety
            "RECITATION" => "stop_sequence", // Content filtered for recitation
            "OTHER" => "end_turn",       // Catch-all for Gemini
            _ => {
                tracing::warn!(reason = %gemini_reason, "Unknown Gemini finish reason, defaulting to end_turn");
                "end_turn"
            }
        }
    }

    /// Determine stop reason with full context awareness
    /// Checks both finish_reason and actual content to make intelligent decision
    fn determine_stop_reason_with_context(
        &self,
        chunk: &GeminiStreamChunk,
        finish_reason: &str,
    ) -> &'static str {
        // Priority 1: Check if there's a function call in this chunk
        if self.has_function_call_in_chunk(chunk) {
            return "tool_use";
        }

        // Priority 2: Check if there's any tool use in content blocks
        if let Some(candidate) = chunk.candidates.first()
            && let Some(content) = &candidate.content
        {
            let has_function_call = content.parts.iter().any(|part| {
                matches!(
                    part,
                    GeminiPart::FunctionCall { .. } | GeminiPart::FunctionCallWithThought { .. }
                )
            });

            if has_function_call {
                return "tool_use";
            }
        }

        // Priority 3: Map the Gemini finish reason
        self.map_finish_reason(finish_reason)
    }

    /// Format tool_use content block start event with input streaming
    fn format_tool_use_start(
        &mut self,
        content_block: &crate::models::claude::ContentBlock,
    ) -> String {
        use crate::models::claude::ContentBlock;

        let index = self.content_block_index;
        self.content_block_index += 1;

        match content_block {
            ContentBlock::ToolUse { id, name, input } => {
                let mut events = String::new();

                // 1. Send content_block_start with empty input
                let start_data = serde_json::json!({
                    "type": "content_block_start",
                    "index": index,
                    "content_block": {
                        "type": "tool_use",
                        "id": id,
                        "name": name,
                        "input": {}  // Start with empty, stream the actual input via delta
                    }
                });
                events.push_str(&format!(
                    "event: content_block_start\ndata: {}\n\n",
                    start_data
                ));

                // 2. Stream the input via content_block_delta with input_json_delta
                let input_str = serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string());
                let delta_data = serde_json::json!({
                    "type": "content_block_delta",
                    "index": index,
                    "delta": {
                        "type": "input_json_delta",
                        "partial_json": input_str
                    }
                });
                events.push_str(&format!(
                    "event: content_block_delta\ndata: {}\n\n",
                    delta_data
                ));

                events
            }
            _ => {
                tracing::error!("Expected ToolUse block");
                String::new()
            }
        }
    }

    /// Check if chunk contains function calls
    fn has_function_call_in_chunk(&self, chunk: &GeminiStreamChunk) -> bool {
        crate::transform::tools::has_function_calls(chunk)
    }

    /// Check if chunk has meaningful content (not just whitespace or empty)
    fn chunk_has_meaningful_content(&self, chunk: &GeminiStreamChunk) -> bool {
        chunk.candidates.first().is_some_and(|candidate| {
            candidate.content.as_ref().is_some_and(|content| {
                content.parts.iter().any(|part| match part {
                    GeminiPart::Text { text } => !text.trim().is_empty(),
                    GeminiPart::TextWithThought { text, .. } => !text.trim().is_empty(),
                    GeminiPart::FunctionCall { .. } => true,
                    GeminiPart::FunctionCallWithThought { .. } => true,
                    _ => false,
                })
            })
        })
    }

    /// Check if headers have been sent
    pub fn is_header_sent(&self) -> bool {
        self.header_sent
    }

    /// Get current token counts
    pub fn token_counts(&self) -> (u32, u32) {
        (self.input_tokens, self.output_tokens)
    }

    /// Format error as SSE event
    pub fn format_error(error_type: &str, message: &str) -> String {
        let data = serde_json::json!({
            "type": "error",
            "error": {
                "type": error_type,
                "message": message
            }
        });
        format!("event: error\ndata: {}\n\n", data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::gemini::{Candidate, GeminiContent, UsageMetadata};

    fn make_text_chunk(text: &str) -> GeminiStreamChunk {
        GeminiStreamChunk {
            candidates: vec![Candidate {
                content: Some(GeminiContent {
                    role: Some("model".to_string()),
                    parts: vec![GeminiPart::Text {
                        text: text.to_string(),
                    }],
                }),
                finish_reason: None,
                safety_ratings: None,
                index: None,
            }],
            usage_metadata: None,
            prompt_feedback: None,
        }
    }

    fn make_finish_chunk() -> GeminiStreamChunk {
        GeminiStreamChunk {
            candidates: vec![Candidate {
                content: None,
                finish_reason: Some("STOP".to_string()),
                safety_ratings: None,
                index: None,
            }],
            usage_metadata: Some(UsageMetadata {
                prompt_token_count: None,
                candidates_token_count: Some(10),
                total_token_count: Some(20),
            }),
            prompt_feedback: None,
        }
    }

    #[test]
    fn test_generate_header_events() {
        let mut event_gen = SSEEventGenerator::new("gemini-3-pro-preview".to_string());

        // Empty chunk with only usage metadata should not send headers
        let chunk = GeminiStreamChunk {
            candidates: vec![],
            usage_metadata: Some(UsageMetadata {
                prompt_token_count: Some(15),
                candidates_token_count: None,
                total_token_count: None,
            }),
            prompt_feedback: None,
        };

        let events = event_gen.generate_events(chunk);
        assert_eq!(events.len(), 0); // No events for empty chunk
        assert!(!event_gen.is_header_sent()); // Headers not sent yet

        // Now send a chunk with actual content
        let content_chunk = make_text_chunk("Hello");
        let events = event_gen.generate_events(content_chunk);

        // Should have headers + delta
        assert!(events.len() >= 3);
        assert!(events.iter().any(|e| e.contains("message_start")));
        assert!(events.iter().any(|e| e.contains("content_block_start")));
        assert!(event_gen.is_header_sent());
    }

    #[test]
    fn test_generate_text_delta() {
        let mut event_gen = SSEEventGenerator::new("gemini-3-pro-preview".to_string());

        let chunk1 = make_text_chunk("Hello");
        let events1 = event_gen.generate_events(chunk1);

        // Should have headers + delta
        assert!(events1.len() >= 3);
        assert!(events1.iter().any(|e| e.contains("content_block_delta")));
        assert!(events1.iter().any(|e| e.contains("Hello")));
    }

    #[test]
    fn test_generate_finish_events() {
        let mut event_gen = SSEEventGenerator::new("gemini-3-pro-preview".to_string());

        // Send header first
        event_gen.generate_events(make_text_chunk("test"));

        // Send finish
        let finish_chunk = make_finish_chunk();
        let events = event_gen.generate_events(finish_chunk);

        assert!(events.iter().any(|e| e.contains("content_block_stop")));
        assert!(events.iter().any(|e| e.contains("message_delta")));
        assert!(events.iter().any(|e| e.contains("message_stop")));
        assert!(events.iter().any(|e| e.contains("end_turn")));
    }

    #[test]
    fn test_token_counting() {
        let mut event_gen = SSEEventGenerator::new("gemini-3-pro-preview".to_string());

        let chunk = GeminiStreamChunk {
            candidates: vec![Candidate {
                content: Some(GeminiContent {
                    role: Some("model".to_string()),
                    parts: vec![GeminiPart::Text {
                        text: "1234567890".to_string(), // 10 chars = ~2-3 tokens
                    }],
                }),
                finish_reason: None,
                safety_ratings: None,
                index: None,
            }],
            usage_metadata: Some(UsageMetadata {
                prompt_token_count: Some(20),
                candidates_token_count: Some(3),
                total_token_count: Some(23),
            }),
            prompt_feedback: None,
        };

        event_gen.generate_events(chunk);
        let (input, output) = event_gen.token_counts();
        assert_eq!(input, 20);
        assert!(output > 0); // Approximate counting
    }

    #[test]
    fn test_finish_reason_mapping() {
        let event_gen = SSEEventGenerator::new("test".to_string());

        assert_eq!(event_gen.map_finish_reason("STOP"), "end_turn");
        assert_eq!(event_gen.map_finish_reason("MAX_TOKENS"), "max_tokens");
        assert_eq!(event_gen.map_finish_reason("SAFETY"), "stop_sequence");
        assert_eq!(event_gen.map_finish_reason("UNKNOWN"), "end_turn");
    }

    #[test]
    fn test_format_error() {
        let error_sse = SSEEventGenerator::format_error("api_error", "Something went wrong");

        assert!(error_sse.contains("event: error"));
        assert!(error_sse.contains("api_error"));
        assert!(error_sse.contains("Something went wrong"));
    }

    #[test]
    fn test_empty_text_skipped() {
        let mut event_gen = SSEEventGenerator::new("gemini-3-pro-preview".to_string());

        let chunk = make_text_chunk("");
        let events = event_gen.generate_events(chunk);

        // Should only have headers, no delta
        assert!(events.iter().all(|e| !e.contains("content_block_delta")));
    }

    #[test]
    fn test_multiple_parts() {
        let mut event_gen = SSEEventGenerator::new("gemini-3-pro-preview".to_string());

        let chunk = GeminiStreamChunk {
            candidates: vec![Candidate {
                content: Some(GeminiContent {
                    role: Some("model".to_string()),
                    parts: vec![
                        GeminiPart::Text {
                            text: "First".to_string(),
                        },
                        GeminiPart::Text {
                            text: "Second".to_string(),
                        },
                    ],
                }),
                finish_reason: None,
                safety_ratings: None,
                index: None,
            }],
            usage_metadata: None,
            prompt_feedback: None,
        };

        let events = event_gen.generate_events(chunk);
        let delta_events: Vec<_> = events
            .iter()
            .filter(|e| e.contains("content_block_delta"))
            .collect();

        assert_eq!(delta_events.len(), 2);
    }
}
