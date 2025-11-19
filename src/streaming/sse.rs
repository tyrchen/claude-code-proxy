use crate::models::gemini::{GeminiPart, GeminiStreamChunk};

/// Converts Gemini chunks to Claude SSE events
pub struct SSEEventGenerator {
    header_sent: bool,
    input_tokens: u32,
    output_tokens: u32,
    model_name: String,
}

impl SSEEventGenerator {
    pub fn new(model_name: String) -> Self {
        Self {
            header_sent: false,
            input_tokens: 0,
            output_tokens: 0,
            model_name,
        }
    }

    pub fn generate_events(&mut self, chunk: GeminiStreamChunk) -> Vec<String> {
        let mut events = Vec::new();

        // Send header events on first chunk
        if !self.header_sent {
            if let Some(usage) = &chunk.usage_metadata {
                self.input_tokens = usage.prompt_token_count.unwrap_or(0);
            }

            events.push(self.format_message_start());
            events.push(self.format_content_block_start());
            self.header_sent = true;
        }

        // Process candidates
        if let Some(candidate) = chunk.candidates.first() {
            // Extract text deltas
            if let Some(content) = &candidate.content {
                for part in &content.parts {
                    if let GeminiPart::Text { text } = part {
                        if !text.is_empty() {
                            events.push(self.format_content_block_delta(text));
                            // Rough token estimation (4 chars ≈ 1 token)
                            self.output_tokens += (text.len() / 4).max(1) as u32;
                        }
                    }
                }
            }

            // Handle finish
            if let Some(finish_reason) = &candidate.finish_reason {
                let stop_reason = self.map_finish_reason(finish_reason);

                if let Some(usage) = &chunk.usage_metadata {
                    self.output_tokens = usage.candidates_token_count.unwrap_or(self.output_tokens);
                }

                events.push(self.format_content_block_stop());
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
                    "output_tokens": 0
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
        format!("event: message_stop\ndata: {{\"type\":\"message_stop\"}}\n\n")
    }

    fn map_finish_reason(&self, gemini_reason: &str) -> &'static str {
        match gemini_reason {
            "STOP" => "end_turn",
            "MAX_TOKENS" => "max_tokens",
            "SAFETY" => "stop_sequence",
            "RECITATION" => "stop_sequence",
            _ => "end_turn",
        }
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
                    role: "model".to_string(),
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
        let mut gen = SSEEventGenerator::new("gemini-2.0-flash-exp".to_string());

        let chunk = GeminiStreamChunk {
            candidates: vec![],
            usage_metadata: Some(UsageMetadata {
                prompt_token_count: Some(15),
                candidates_token_count: None,
                total_token_count: None,
            }),
            prompt_feedback: None,
        };

        let events = gen.generate_events(chunk);
        assert_eq!(events.len(), 2); // message_start + content_block_start
        assert!(events[0].contains("message_start"));
        assert!(events[1].contains("content_block_start"));
        assert!(gen.is_header_sent());
    }

    #[test]
    fn test_generate_text_delta() {
        let mut gen = SSEEventGenerator::new("gemini-2.0-flash-exp".to_string());

        let chunk1 = make_text_chunk("Hello");
        let events1 = gen.generate_events(chunk1);

        // Should have headers + delta
        assert!(events1.len() >= 3);
        assert!(events1.iter().any(|e| e.contains("content_block_delta")));
        assert!(events1.iter().any(|e| e.contains("Hello")));
    }

    #[test]
    fn test_generate_finish_events() {
        let mut gen = SSEEventGenerator::new("gemini-2.0-flash-exp".to_string());

        // Send header first
        gen.generate_events(make_text_chunk("test"));

        // Send finish
        let finish_chunk = make_finish_chunk();
        let events = gen.generate_events(finish_chunk);

        assert!(events.iter().any(|e| e.contains("content_block_stop")));
        assert!(events.iter().any(|e| e.contains("message_delta")));
        assert!(events.iter().any(|e| e.contains("message_stop")));
        assert!(events.iter().any(|e| e.contains("end_turn")));
    }

    #[test]
    fn test_token_counting() {
        let mut gen = SSEEventGenerator::new("gemini-2.0-flash-exp".to_string());

        let chunk = GeminiStreamChunk {
            candidates: vec![Candidate {
                content: Some(GeminiContent {
                    role: "model".to_string(),
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
                candidates_token_count: None,
                total_token_count: None,
            }),
            prompt_feedback: None,
        };

        gen.generate_events(chunk);
        let (input, output) = gen.token_counts();
        assert_eq!(input, 20);
        assert!(output > 0); // Approximate counting
    }

    #[test]
    fn test_finish_reason_mapping() {
        let gen = SSEEventGenerator::new("test".to_string());

        assert_eq!(gen.map_finish_reason("STOP"), "end_turn");
        assert_eq!(gen.map_finish_reason("MAX_TOKENS"), "max_tokens");
        assert_eq!(gen.map_finish_reason("SAFETY"), "stop_sequence");
        assert_eq!(gen.map_finish_reason("UNKNOWN"), "end_turn");
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
        let mut gen = SSEEventGenerator::new("gemini-2.0-flash-exp".to_string());

        let chunk = make_text_chunk("");
        let events = gen.generate_events(chunk);

        // Should only have headers, no delta
        assert!(events.iter().all(|e| !e.contains("content_block_delta")));
    }

    #[test]
    fn test_multiple_parts() {
        let mut gen = SSEEventGenerator::new("gemini-2.0-flash-exp".to_string());

        let chunk = GeminiStreamChunk {
            candidates: vec![Candidate {
                content: Some(GeminiContent {
                    role: "model".to_string(),
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

        let events = gen.generate_events(chunk);
        let delta_events: Vec<_> = events
            .iter()
            .filter(|e| e.contains("content_block_delta"))
            .collect();

        assert_eq!(delta_events.len(), 2);
    }
}
