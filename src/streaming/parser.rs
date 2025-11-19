use crate::error::Result;
use crate::models::gemini::GeminiStreamChunk;
use bytes::{Buf, BytesMut};

/// Stateful parser for Gemini's chunked JSON array stream
pub struct StreamingJsonParser {
    buffer: BytesMut,
    array_started: bool,
}

impl StreamingJsonParser {
    pub fn new() -> Self {
        Self {
            buffer: BytesMut::with_capacity(8192),
            array_started: false,
        }
    }

    /// Feed new data and extract complete JSON objects
    pub fn feed(&mut self, chunk: &[u8]) -> Result<Vec<GeminiStreamChunk>> {
        self.buffer.extend_from_slice(chunk);
        self.extract_objects()
    }

    fn extract_objects(&mut self) -> Result<Vec<GeminiStreamChunk>> {
        let mut results = Vec::new();

        loop {
            // Skip leading whitespace, commas, and array brackets
            self.skip_noise();

            if self.buffer.is_empty() {
                break;
            }

            // Check for array end
            if self.buffer[0] == b']' {
                self.buffer.advance(1);
                continue;
            }

            // Find complete JSON object
            if let Some(obj_end) = self.find_object_boundary() {
                let obj_bytes = self.buffer.split_to(obj_end);

                // Parse JSON object
                match serde_json::from_slice::<GeminiStreamChunk>(&obj_bytes) {
                    Ok(chunk) => results.push(chunk),
                    Err(e) => {
                        eprintln!("Failed to parse Gemini chunk: {}", e);
                        eprintln!("Raw bytes: {}", String::from_utf8_lossy(&obj_bytes));
                        // Continue processing instead of failing
                    }
                }
            } else {
                // Incomplete object, wait for more data
                break;
            }
        }

        Ok(results)
    }

    fn skip_noise(&mut self) {
        while !self.buffer.is_empty() {
            match self.buffer[0] {
                b'[' => {
                    self.array_started = true;
                    self.buffer.advance(1);
                }
                b',' | b' ' | b'\n' | b'\r' | b'\t' => {
                    self.buffer.advance(1);
                }
                _ => break,
            }
        }
    }

    fn find_object_boundary(&self) -> Option<usize> {
        let mut depth = 0;
        let mut in_string = false;
        let mut escaped = false;

        for (i, &byte) in self.buffer.iter().enumerate() {
            if in_string {
                if escaped {
                    escaped = false;
                } else {
                    match byte {
                        b'\\' => escaped = true,
                        b'"' => in_string = false,
                        _ => {}
                    }
                }
            } else {
                match byte {
                    b'"' => in_string = true,
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(i + 1);
                        }
                    }
                    _ => {}
                }
            }
        }

        None
    }

    /// Reset the parser state (useful for connection reuse)
    pub fn reset(&mut self) {
        self.buffer.clear();
        if self.buffer.capacity() > 65536 {
            // 64KB max, reallocate if too large
            self.buffer = BytesMut::with_capacity(8192);
        }
        self.array_started = false;
    }
}

impl Default for StreamingJsonParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_complete_object() {
        let mut parser = StreamingJsonParser::new();
        let data = br#"[{"candidates":[]}]"#;
        let chunks = parser.feed(data).unwrap();
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_parse_incomplete_chunks() {
        let mut parser = StreamingJsonParser::new();

        // First chunk is incomplete
        let chunk1 = br#"[{"candidates":[{"content":{"parts":[{"text":"H"#;
        let results1 = parser.feed(chunk1).unwrap();
        assert_eq!(results1.len(), 0);

        // Second chunk completes first object
        let chunk2 = br#"ello"}],"role":"model"}}]}]"#;
        let results2 = parser.feed(chunk2).unwrap();
        assert_eq!(results2.len(), 1);
    }

    #[test]
    fn test_multiple_objects() {
        let mut parser = StreamingJsonParser::new();
        let data = br#"[{"candidates":[]},{"candidates":[]}]"#;
        let chunks = parser.feed(data).unwrap();
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn test_escaped_strings() {
        let mut parser = StreamingJsonParser::new();
        let data = br#"[{"candidates":[{"content":{"parts":[{"text":"He said \"hello\""}],"role":"model"}}]}]"#;
        let chunks = parser.feed(data).unwrap();
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_whitespace_handling() {
        let mut parser = StreamingJsonParser::new();
        let data = b"[\n  {\"candidates\": []}\n  ,\n  {\"candidates\": []}\n]";
        let chunks = parser.feed(data).unwrap();
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn test_streaming_with_usage_metadata() {
        let mut parser = StreamingJsonParser::new();
        let data = br#"[
            {"candidates":[{"content":{"parts":[{"text":"Hello"}],"role":"model"}}],"usageMetadata":{"promptTokenCount":10}},
            {"candidates":[{"content":{"parts":[{"text":" world"}],"role":"model"}}]}
        ]"#;
        let chunks = parser.feed(data).unwrap();
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].usage_metadata.is_some());
    }

    #[test]
    fn test_parser_reset() {
        let mut parser = StreamingJsonParser::new();
        let data = br#"[{"candidates":[]}]"#;

        parser.feed(data).unwrap();
        assert!(parser.array_started);

        parser.reset();
        assert!(!parser.array_started);
        assert!(parser.buffer.is_empty());
    }

    #[test]
    fn test_object_split_across_multiple_feeds() {
        let mut parser = StreamingJsonParser::new();

        let chunk1 = b"[{\"candidates\":[{\"content\":";
        let chunk2 = b"{\"parts\":[{\"text\":";
        let chunk3 = b"\"test\"}],\"role\":\"model\"}}]}]";

        assert_eq!(parser.feed(chunk1).unwrap().len(), 0);
        assert_eq!(parser.feed(chunk2).unwrap().len(), 0);
        let results = parser.feed(chunk3).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_nested_objects() {
        let mut parser = StreamingJsonParser::new();
        let data =
            br#"[{"candidates":[{"content":{"parts":[{"text":"nested"}],"role":"model"}}]}]"#;
        let chunks = parser.feed(data).unwrap();
        assert_eq!(chunks.len(), 1);
    }
}
