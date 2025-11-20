use crate::error::Result;
use crate::models::gemini::GeminiStreamChunk;
use bytes::{Buf, BytesMut};

/// Buffer for accumulating partial tool/function call input JSON
/// Handles incremental streaming of function arguments
#[derive(Debug, Clone)]
pub struct ToolInputBuffer {
    /// Accumulated partial JSON for tool input
    pub partial_json: String,
    /// Tool/function name
    pub tool_name: String,
    /// Tool use ID (Claude format)
    pub tool_id: Option<String>,
    /// Whether this buffer is complete
    pub is_complete: bool,
}

impl ToolInputBuffer {
    pub fn new(tool_name: String) -> Self {
        Self {
            partial_json: String::new(),
            tool_name,
            tool_id: None,
            is_complete: false,
        }
    }

    /// Append a chunk of JSON to the buffer
    pub fn append(&mut self, chunk: &str) {
        self.partial_json.push_str(chunk);
    }

    /// Try to parse the accumulated JSON, returning the parsed value if complete
    pub fn try_parse(&self) -> Option<serde_json::Value> {
        serde_json::from_str(&self.partial_json).ok()
    }

    /// Mark as complete and try final parse
    pub fn finalize(&mut self) -> Result<serde_json::Value> {
        self.is_complete = true;
        serde_json::from_str(&self.partial_json).map_err(|e| {
            crate::error::ProxyError::TransformationError(format!(
                "Failed to parse complete tool input JSON for {}: {} - JSON was: {}",
                self.tool_name, e, self.partial_json
            ))
        })
    }

    /// Get the current size of buffered data
    pub fn size(&self) -> usize {
        self.partial_json.len()
    }
}

/// Stateful parser for Gemini's chunked JSON array stream
/// Enhanced with tool input buffering for incremental function call parsing
pub struct StreamingJsonParser {
    buffer: BytesMut,
    array_started: bool,
    /// Optional tool input buffer for accumulating partial function args
    tool_input_buffer: Option<ToolInputBuffer>,
}

impl StreamingJsonParser {
    pub fn new() -> Self {
        Self {
            buffer: BytesMut::with_capacity(8192),
            array_started: false,
            tool_input_buffer: None,
        }
    }

    /// Start buffering tool input for a function call
    pub fn start_tool_input(&mut self, tool_name: String, tool_id: Option<String>) {
        let mut buffer = ToolInputBuffer::new(tool_name);
        buffer.tool_id = tool_id;
        self.tool_input_buffer = Some(buffer);
        tracing::debug!("Started tool input buffering");
    }

    /// Append to current tool input buffer
    pub fn append_tool_input(&mut self, chunk: &str) -> Option<serde_json::Value> {
        if let Some(buffer) = &mut self.tool_input_buffer {
            buffer.append(chunk);
            tracing::debug!(size = buffer.size(), "Appended to tool input buffer");
            // Try to parse, return if complete
            buffer.try_parse()
        } else {
            None
        }
    }

    /// Finalize and extract tool input buffer
    pub fn finalize_tool_input(
        &mut self,
    ) -> Result<Option<(String, Option<String>, serde_json::Value)>> {
        if let Some(mut buffer) = self.tool_input_buffer.take() {
            let parsed = buffer.finalize()?;
            tracing::debug!(
                tool_name = %buffer.tool_name,
                size = buffer.size(),
                "Finalized tool input buffer"
            );
            Ok(Some((buffer.tool_name, buffer.tool_id, parsed)))
        } else {
            Ok(None)
        }
    }

    /// Check if currently buffering tool input
    pub fn is_buffering_tool_input(&self) -> bool {
        self.tool_input_buffer.is_some()
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
        self.tool_input_buffer = None;
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
