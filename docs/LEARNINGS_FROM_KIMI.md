# Learnings from Kimi Logs - Applying to Gemini Proxy

Based on analyzing `/tmp/kimi.log`, here are key insights about how Claude-native APIs work and what we can apply to improve our Gemini transformation.

## 1. Native Claude Request Format

### What Kimi Shows Us (Pure Claude Format):

```json
{
  "model": "kimi-k2-thinking-turbo",
  "max_tokens": 32000,
  "messages": [
    {
      "role": "user",
      "content": [
        {
          "type": "text",
          "text": "Hello"
        }
      ]
    },
    {
      "role": "assistant",
      "content": [
        {
          "type": "tool_use",
          "id": "TodoWrite_0",
          "name": "TodoWrite",
          "input": {
            "todos": [...]
          }
        }
      ]
    },
    {
      "role": "user",
      "content": [
        {
          "type": "tool_result",
          "tool_use_id": "TodoWrite_0",
          "content": "Todos have been modified successfully..."
        }
      ]
    }
  ],
  "tools": [
    {
      "name": "TodoWrite",
      "description": "...",
      "input_schema": {
        "type": "object",
        "properties": {...},
        "required": [...]
      }
    }
  ],
  "stream": true,
  "system": [
    {
      "type": "text",
      "text": "You are Claude Code..."
    }
  ]
}
```

### Key Observations:

1. **System Prompts**: Array of text blocks, not a single string
2. **Content Blocks**: Always use `type` field explicitly
3. **Tool Schema**: Uses JSON Schema format with `input_schema`
4. **Tool Use Flow**:
   - Assistant returns `tool_use` blocks
   - User responds with `tool_result` blocks
   - `tool_use_id` links them together

## 2. Native Claude Response Format (SSE)

### What Kimi Sends Back:

```
event: message_start
data: {"type":"message_start","message":{"id":"msg_...","model":"kimi-k2-thinking-turbo"...}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}

event: content_block_start
data: {"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"TodoWrite_0","name":"TodoWrite","input":{}}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"todos\":"}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"[{\"content\":"}}

event: content_block_stop
data: {"type":"content_block_stop","index":1}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":150}}

event: message_stop
data: {"type":"message_stop"}
```

### Key Observations:

1. **Progressive Rendering**: Tool inputs stream as `input_json_delta` chunks
2. **Multiple Content Blocks**: Index-based, can mix text and tool_use
3. **Structured Events**: Clear lifecycle (start → delta → stop)
4. **Usage Tracking**: Token counts in `message_delta` event

## 3. Critical Differences from Gemini

| Aspect | Claude (Kimi) | Gemini | Impact on Transformation |
|--------|---------------|--------|-------------------------|
| **Tool Input Streaming** | `input_json_delta` | Complete in `functionCall` | Need to buffer partial JSON |
| **Tool Result Format** | `tool_result` with `tool_use_id` | `functionResponse` with `name` | ID mapping required |
| **Content Indexing** | Index per content block | Flat array of parts | Reordering needed |
| **Stop Reasons** | `tool_use`, `end_turn` | `STOP`, `MAX_TOKENS` | Translation table needed |
| **System Prompts** | Array of typed objects | Single `instruction` field | Flattening required |

## 4. TodoWrite Handling - What Works Well

### Request (Tool Definition):

```json
{
  "name": "TodoWrite",
  "description": "Use this tool to create and manage...",
  "input_schema": {
    "type": "object",
    "properties": {
      "todos": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "content": {"type": "string"},
            "status": {"enum": ["pending", "in_progress", "completed"]},
            "activeForm": {"type": "string"}
          },
          "required": ["content", "status", "activeForm"]
        }
      }
    },
    "required": ["todos"]
  }
}
```

**Kimi Understanding**: Perfect - uses schema exactly as defined

### Response (Tool Use):

```json
{
  "type": "tool_use",
  "id": "TodoWrite_15",
  "name": "TodoWrite",
  "input": {
    "todos": [
      {
        "content": "Review the git diff",
        "status": "completed",
        "activeForm": "Reviewed git diff"
      },
      {
        "content": "Analyze architecture",
        "status": "in_progress",
        "activeForm": "Analyzing architecture"
      }
    ]
  }
}
```

**Kimi Generation**: Clean, follows schema perfectly

### Tool Result (User Response):

```json
{
  "type": "tool_result",
  "tool_use_id": "TodoWrite_15",
  "content": "Todos have been modified successfully. Ensure that you continue to use the todo list to track your progress."
}
```

**Kimi Handling**: Processes immediately, continues conversation

## 5. Improvements for Gemini Transformation

### 5.1 Enhanced Tool ID Mapping

**Current Issue**: We map `tool_use_id` to `function_name`, but Gemini doesn't preserve IDs.

**Learning from Kimi**: Tool IDs are critical for multi-turn conversations.

**Improvement**:
```rust
// src/state.rs - Enhanced mapping
pub struct ToolMapping {
    pub tool_use_id: String,        // Claude's ID (e.g., "TodoWrite_15")
    pub function_name: String,       // Gemini's function name
    pub request_index: usize,        // Which request in conversation
    pub conversation_id: String,     // Track across multiple requests
}

// Store more context for better recovery
impl ConversationState {
    pub fn register_with_context(&self, id: String, name: String, context: ToolContext) {
        // Store not just mapping, but full context
    }
}
```

### 5.2 Better Streaming JSON Buffering

**Learning from Kimi**: Tool inputs stream incrementally with `input_json_delta`.

**Current Gemini Issue**: Gemini sends complete `functionCall` objects, but we might get partial frames.

**Improvement**:
```rust
// src/streaming/parser.rs
pub struct ToolInputBuffer {
    partial_json: String,
    tool_id: String,
    tool_name: String,
}

impl StreamingJsonParser {
    fn accumulate_tool_input(&mut self, delta: &str) -> Option<ToolInput> {
        self.buffer.partial_json.push_str(delta);

        // Try to parse, return None if incomplete
        match serde_json::from_str(&self.buffer.partial_json) {
            Ok(input) => Some(input),
            Err(_) if delta.ends_with('}') => {
                // Last chunk, force parse or error
            }
            Err(_) => None, // Still accumulating
        }
    }
}
```

### 5.3 Multi-Block Content Handling

**Learning from Kimi**: Responses can have multiple content blocks with different indices.

**Current Approach**: We flatten everything in transformation.

**Improvement**:
```rust
// src/transform/response.rs
pub struct ContentBlock {
    index: usize,
    block_type: ContentBlockType,
    content: serde_json::Value,
}

impl SSEEventGenerator {
    fn handle_mixed_content(&mut self, gemini_parts: Vec<GeminiPart>) -> Vec<ContentBlock> {
        let mut blocks = Vec::new();
        let mut current_index = 0;

        for part in gemini_parts {
            match part {
                GeminiPart::Text(t) => {
                    blocks.push(ContentBlock {
                        index: current_index,
                        block_type: ContentBlockType::Text,
                        content: json!({"text": t}),
                    });
                    current_index += 1;
                }
                GeminiPart::FunctionCall(fc) => {
                    blocks.push(ContentBlock {
                        index: current_index,
                        block_type: ContentBlockType::ToolUse,
                        content: json!({
                            "id": self.generate_tool_id(&fc.name),
                            "name": fc.name,
                            "input": fc.args,
                        }),
                    });
                    current_index += 1;
                }
            }
        }

        blocks
    }
}
```

### 5.4 System Prompt Array Support

**Learning from Kimi**: System prompts are arrays of typed objects.

**Current Issue**: We concatenate into single string for Gemini.

**Improvement**:
```rust
// src/transform/request.rs
fn preserve_system_structure(system: Vec<SystemBlock>) -> GeminiInstruction {
    // Instead of just concatenating, preserve structure in metadata
    let concatenated = system.iter()
        .map(|block| &block.text)
        .collect::<Vec<_>>()
        .join("\n\n");

    GeminiInstruction {
        text: concatenated,
        // Store original structure for potential round-trip
        metadata: Some(json!({
            "original_blocks": system.len(),
            "block_types": system.iter().map(|b| &b.type_).collect::<Vec<_>>(),
        })),
    }
}
```

### 5.5 Robust Stop Reason Mapping

**Learning from Kimi**: Claude has specific stop reasons that affect tool handling.

**Current Mapping**:
```rust
// src/streaming/sse.rs
fn map_finish_reason(gemini_reason: &str) -> &str {
    match gemini_reason {
        "STOP" => "end_turn",
        "MAX_TOKENS" => "max_tokens",
        _ => "end_turn",
    }
}
```

**Improvement**:
```rust
fn map_finish_reason_with_context(
    gemini_reason: &str,
    has_function_call: bool,
    content: &[GeminiPart],
) -> &str {
    // Check if response ends with function call
    if has_function_call || content.iter().any(|p| matches!(p, GeminiPart::FunctionCall(_))) {
        return "tool_use";
    }

    match gemini_reason {
        "STOP" => "end_turn",
        "MAX_TOKENS" => "max_tokens",
        "SAFETY" => "stop_sequence", // Closest equivalent
        "RECITATION" => "stop_sequence",
        _ => {
            tracing::warn!("Unknown finish reason: {}", gemini_reason);
            "end_turn"
        }
    }
}
```

## 6. Testing Improvements

### 6.1 Add Round-Trip Tests

**Learning**: Kimi's pure forwarding shows what "perfect" looks like.

**New Test**:
```rust
#[test]
fn test_todowrite_roundtrip() {
    // 1. Start with Claude request with TodoWrite tool
    let claude_req = ClaudeRequest { ... };

    // 2. Transform to Gemini
    let gemini_req = transform_request(claude_req.clone());

    // 3. Simulate Gemini response with function call
    let gemini_response = GeminiResponse {
        function_call: Some(FunctionCall {
            name: "TodoWrite",
            args: json!({"todos": [...]})
        })
    };

    // 4. Transform back to Claude SSE
    let sse_events = transform_to_sse(gemini_response);

    // 5. Verify tool_use_id is generated and mappable
    assert!(sse_events.iter().any(|e| e.contains("tool_use")));

    // 6. Create tool_result response
    let tool_result = ClaudeRequest {
        messages: vec![
            Message {
                role: "user",
                content: vec![
                    ContentBlock::ToolResult {
                        tool_use_id: extracted_id,
                        content: "Success"
                    }
                ]
            }
        ]
    };

    // 7. Transform again and verify mapping works
    let second_gemini = transform_request(tool_result);
    assert!(second_gemini.contents.iter().any(|c|
        matches!(c.parts.first(), Some(GeminiPart::FunctionResponse(_)))
    ));
}
```

### 6.2 Streaming Chunk Tests

**New Test**:
```rust
#[test]
fn test_partial_function_call_streaming() {
    let mut parser = StreamingJsonParser::new();
    let mut generator = SSEEventGenerator::new();

    // Simulate chunked Gemini response
    let chunks = vec![
        r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"TodoWrite","#,
        r#""args":{"todos":[{"content":"Test","status":"pending","#,
        r#""activeForm":"Testing"}]}}}]}}]}"#,
    ];

    for chunk in chunks {
        let parsed = parser.feed(chunk.as_bytes()).unwrap();
        for gemini_chunk in parsed {
            let events = generator.generate_events(gemini_chunk);
            // Verify we get proper content_block_delta events
        }
    }
}
```

## 7. Logging Improvements for Gemini

### Apply Kimi's Logging Structure

**Add to GeminiClient**:
```rust
// src/client/gemini.rs
impl GeminiClient {
    fn log_transformation_details(&self, claude_req: &ClaudeRequest, gemini_req: &GeminiRequest) {
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/gemini.log")
        {
            let _ = writeln!(file, "\n{}", "=".repeat(80));
            let _ = writeln!(file, "=== TRANSFORMATION DETAILS ===");
            let _ = writeln!(file, "{}", "=".repeat(80));

            // Show side-by-side comparison
            let _ = writeln!(file, "\n--- ORIGINAL CLAUDE REQUEST ---");
            let _ = writeln!(file, "{}", serde_json::to_string_pretty(claude_req).unwrap());

            let _ = writeln!(file, "\n--- TRANSFORMED GEMINI REQUEST ---");
            let _ = writeln!(file, "{}", serde_json::to_string_pretty(gemini_req).unwrap());

            // Highlight key transformations
            let _ = writeln!(file, "\n--- TRANSFORMATION SUMMARY ---");
            let _ = writeln!(file, "System blocks: {} -> 1", claude_req.system.len());
            let _ = writeln!(file, "Messages: {}", claude_req.messages.len());
            let _ = writeln!(file, "Tools: {}", claude_req.tools.as_ref().map(|t| t.len()).unwrap_or(0));
        }
    }
}
```

## 8. Documentation Improvements

### Add Kimi Comparison Section to README

```markdown
## How It Works: Gemini Transformation vs Kimi Forwarding

### Kimi (Pure Forwarding)
```
Claude Code → [Kimi Proxy] → Kimi API
              (no changes)

Request: Native Claude format
Response: Native Claude SSE events
Tools: Native tool_use/tool_result
```

### Gemini (With Transformation)
```
Claude Code → [Gemini Proxy] → Gemini API
              ↓ Transform     ↓ Transform back
              Claude → Gemini → Claude SSE

Request: Claude → Gemini format conversion
Response: Gemini JSON → Claude SSE events
Tools: tool_use → function_call → tool_use
```

## Summary of Action Items

1. ✅ **Logging**: Added comprehensive Kimi logging (done)
2. **State Management**: Enhance tool ID mapping with context
3. **Streaming**: Improve partial JSON buffering
4. **Content Blocks**: Better multi-block handling
5. **Testing**: Add round-trip and streaming tests
6. **Documentation**: Add transformation comparison guide

The key insight from Kimi is that **simplicity wins** - when possible, pure forwarding is more reliable than transformation. For Gemini, we should aim to make transformations as transparent and reversible as possible.
