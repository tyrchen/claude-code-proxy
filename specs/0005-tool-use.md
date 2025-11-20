# Spec 0005: Tool Use / Function Calling Support

**Status**: Draft
**Author**: Claude Code Proxy Team
**Created**: 2025-11-19
**Updated**: 2025-11-19

## Executive Summary

This specification defines the architecture and implementation plan for bidirectional tool calling transformation between Claude's Tool Use API and Gemini's Function Calling API. This is a **critical feature** required for Claude Code to work properly through the proxy.

## Problem Statement

### Current State

The proxy currently **drops tool definitions** during request transformation:
- Claude Code sends tool schemas (TodoWrite, Task, Bash, Read, Edit, etc.)
- Proxy transforms request but **omits tools field**
- Gemini receives tool instructions in system prompt but no function definitions
- Gemini attempts to call tools based on instructions alone
- Result: `MALFORMED_FUNCTION_CALL` errors and complete failure

### Impact

Without tool support:
- ❌ TodoWrite fails (task tracking broken)
- ❌ Task spawning fails (subagent system broken)
- ❌ AskUserQuestion fails (no user interaction)
- ❌ All Claude Code tool features unusable
- ❌ Proxy is only 30% functional

### Required Outcome

Full bidirectional tool transformation:
1. **Request**: Claude tool definitions → Gemini function declarations
2. **Response**: Gemini function calls → Claude tool use blocks
3. **Follow-up**: Claude tool results → Gemini function responses

## Architecture Design

### 1. Format Comparison

#### Claude Tool Definition Format

```json
{
  "name": "TodoWrite",
  "description": "Create and manage task lists",
  "input_schema": {
    "type": "object",
    "properties": {
      "todos": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "content": {"type": "string"},
            "status": {"type": "string", "enum": ["pending", "in_progress", "completed"]},
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

#### Gemini Function Declaration Format

```json
{
  "name": "TodoWrite",
  "description": "Create and manage task lists",
  "parameters": {
    "type": "object",
    "properties": {
      "todos": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "content": {"type": "string"},
            "status": {"type": "string", "enum": ["pending", "in_progress", "completed"]},
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

**Key Difference**: Claude uses `input_schema`, Gemini uses `parameters`. The schema structure is **nearly identical** (both use JSON Schema).

### 2. Response Flow Comparison

#### Claude Tool Use Response

```json
{
  "type": "message",
  "role": "assistant",
  "content": [
    {
      "type": "tool_use",
      "id": "toolu_01A09q90qw90lq917835lq9",
      "name": "TodoWrite",
      "input": {
        "todos": [
          {"content": "Fix bug", "status": "pending", "activeForm": "Fixing bug"}
        ]
      }
    }
  ],
  "stop_reason": "tool_use"
}
```

#### Gemini Function Call Response

```json
{
  "candidates": [{
    "content": {
      "parts": [{
        "functionCall": {
          "name": "TodoWrite",
          "args": {
            "todos": [
              {"content": "Fix bug", "status": "pending", "activeForm": "Fixing bug"}
            ]
          }
        }
      }],
      "role": "model"
    },
    "finishReason": "STOP"
  }]
}
```

#### Claude Tool Result (Follow-up Request)

```json
{
  "role": "user",
  "content": [
    {
      "type": "tool_result",
      "tool_use_id": "toolu_01A09q90qw90lq917835lq9",
      "content": "Task added successfully"
    }
  ]
}
```

#### Gemini Function Response (Follow-up Request)

```json
{
  "role": "user",
  "parts": [{
    "functionResponse": {
      "name": "TodoWrite",
      "response": {
        "result": "Task added successfully"
      }
    }
  }]
}
```

### 3. Multi-Turn Conversation Flow

```
User Request → Claude Code
    ↓
Claude Request (with tools) → Proxy
    ↓
Gemini Request (with functions) → Gemini API
    ↓
Gemini Response (function call) → Proxy
    ↓
Claude Response (tool_use) → Claude Code
    ↓
Claude Code executes tool locally
    ↓
Claude Request (tool_result) → Proxy
    ↓
Gemini Request (functionResponse) → Gemini API
    ↓
Gemini Response (final answer) → Proxy
    ↓
Claude Response (text) → Claude Code
```

## Implementation Plan

### Phase 1: Data Models (Week 1)

#### 1.1 Extend Claude Models

**File**: `src/models/claude.rs`

```rust
// Add to ClaudeRequest
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClaudeRequest {
    // ... existing fields ...

    /// Tool definitions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ClaudeTool>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClaudeTool {
    pub name: String,
    pub description: String,
    pub input_schema: JsonSchema,
}

/// JSON Schema definition (supports full spec)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonSchema {
    #[serde(rename = "type")]
    pub schema_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<std::collections::HashMap<String, Box<JsonSchema>>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "enum")]
    pub enum_values: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<JsonSchema>>,

    // Support for additional JSON Schema fields
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

// Extend ContentBlock for tool use/result
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text {
        text: String,
    },

    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },

    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}
```

#### 1.2 Extend Gemini Models

**File**: `src/models/gemini.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiRequest {
    // ... existing fields ...

    /// Tool declarations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<GeminiTool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiTool {
    pub function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiFunctionDeclaration {
    pub name: String,
    pub description: String,
    pub parameters: JsonSchema, // Reuse from claude.rs
}

// Extend GeminiPart for function calls/responses
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GeminiPart {
    Text { text: String },
    TextWithThought {
        text: String,
        #[serde(rename = "thoughtSignature")]
        thought_signature: String,
    },
    InlineData { inline_data: InlineData },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: FunctionCall,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: FunctionResponse,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionCall {
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionResponse {
    pub name: String,
    pub response: serde_json::Value,
}
```

### Phase 2: Tool Transformation (Week 1-2)

#### 2.1 Request Transformation

**File**: `src/transform/tools.rs` (new)

```rust
use crate::error::{ProxyError, Result};
use crate::models::claude::{ClaudeTool, JsonSchema};
use crate::models::gemini::{GeminiFunctionDeclaration, GeminiTool};

/// Transform Claude tools to Gemini function declarations
pub fn transform_tools(claude_tools: Vec<ClaudeTool>) -> Result<Vec<GeminiTool>> {
    let function_declarations = claude_tools
        .into_iter()
        .map(transform_tool)
        .collect::<Result<Vec<_>>>()?;

    Ok(vec![GeminiTool {
        function_declarations,
    }])
}

/// Transform single tool definition
fn transform_tool(tool: ClaudeTool) -> Result<GeminiFunctionDeclaration> {
    Ok(GeminiFunctionDeclaration {
        name: tool.name,
        description: tool.description,
        parameters: tool.input_schema, // Schema format is compatible!
    })
}

/// Transform Claude tool result to Gemini function response
pub fn transform_tool_result(
    tool_result: &ContentBlock,
) -> Result<GeminiPart> {
    match tool_result {
        ContentBlock::ToolResult { tool_use_id, content, is_error } => {
            // Need to find the original function name from conversation history
            // This requires maintaining tool_use_id -> function_name mapping
            Ok(GeminiPart::FunctionResponse {
                function_response: FunctionResponse {
                    name: /* lookup from state */,
                    response: json!({
                        "result": content,
                        "error": is_error.unwrap_or(false)
                    }),
                }
            })
        }
        _ => Err(ProxyError::TransformationError(
            "Expected tool_result block".into()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_simple_tool() {
        let tool = ClaudeTool {
            name: "get_weather".to_string(),
            description: "Get weather for a location".to_string(),
            input_schema: JsonSchema {
                schema_type: "object".to_string(),
                properties: Some(hashmap! {
                    "location".to_string() => Box::new(JsonSchema {
                        schema_type: "string".to_string(),
                        description: Some("City name".to_string()),
                        ..Default::default()
                    })
                }),
                required: Some(vec!["location".to_string()]),
                ..Default::default()
            },
        };

        let result = transform_tool(tool).unwrap();
        assert_eq!(result.name, "get_weather");
        assert_eq!(result.parameters.schema_type, "object");
    }
}
```

#### 2.2 Response Transformation

**File**: `src/transform/tools.rs` (continued)

```rust
/// Transform Gemini function call to Claude tool use
pub fn transform_function_call(
    function_call: &FunctionCall,
) -> Result<ContentBlock> {
    // Generate unique tool_use_id
    let id = format!("toolu_{}", uuid::Uuid::new_v4().simple());

    Ok(ContentBlock::ToolUse {
        id,
        name: function_call.name.clone(),
        input: function_call.args.clone(),
    })
}

/// Detect if Gemini response contains function calls
pub fn has_function_calls(chunk: &GeminiStreamChunk) -> bool {
    chunk.candidates.iter().any(|c| {
        c.content.as_ref().map_or(false, |content| {
            content.parts.iter().any(|part| {
                matches!(part, GeminiPart::FunctionCall { .. })
            })
        })
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_transform_function_call() {
        let fc = FunctionCall {
            name: "TodoWrite".to_string(),
            args: json!({
                "todos": [
                    {
                        "content": "Test task",
                        "status": "pending",
                        "activeForm": "Testing"
                    }
                ]
            }),
        };

        let block = transform_function_call(&fc).unwrap();
        match block {
            ContentBlock::ToolUse { name, input, .. } => {
                assert_eq!(name, "TodoWrite");
                assert!(input["todos"].is_array());
            }
            _ => panic!("Expected ToolUse"),
        }
    }
}
```

### Phase 3: State Management (Week 2)

**Problem**: Need to map `tool_use_id` ↔ `function_name` across turns.

**Solution**: Maintain conversation state in the handler.

**File**: `src/state.rs` (new)

```rust
use dashmap::DashMap;
use std::sync::Arc;

/// Conversation state for tracking tool calls
#[derive(Clone)]
pub struct ConversationState {
    /// Maps tool_use_id -> function_name
    tool_mappings: Arc<DashMap<String, String>>,
}

impl ConversationState {
    pub fn new() -> Self {
        Self {
            tool_mappings: Arc::new(DashMap::new()),
        }
    }

    /// Store mapping when transforming Gemini function call to Claude tool use
    pub fn register_tool_use(&self, tool_use_id: String, function_name: String) {
        self.tool_mappings.insert(tool_use_id, function_name);
    }

    /// Retrieve function name when transforming Claude tool result to Gemini
    pub fn get_function_name(&self, tool_use_id: &str) -> Option<String> {
        self.tool_mappings.get(tool_use_id).map(|v| v.clone())
    }

    /// Clean up old mappings (optional, for memory management)
    pub fn cleanup(&self) {
        // Remove entries older than X minutes
        // Implementation depends on tracking timestamps
    }
}

// Thread-safe singleton for development
// In production, use per-session state
lazy_static::lazy_static! {
    pub static ref GLOBAL_STATE: ConversationState = ConversationState::new();
}
```

**Note**: For production, implement per-session state using session IDs from Claude Code or request headers.

### Phase 4: Integration (Week 2-3)

#### 4.1 Update Request Handler

**File**: `src/transform/request.rs`

```rust
// Add to transform_request function
pub fn transform_request(claude_req: ClaudeRequest) -> Result<GeminiRequest> {
    // ... existing validation ...

    // Transform tools
    let tools = claude_req
        .tools
        .map(|t| transform_tools(t))
        .transpose()?;

    // ... existing message transformation ...

    Ok(GeminiRequest {
        contents,
        system_instruction,
        generation_config: Some(gen_config),
        safety_settings: None,
        tools, // Add tools field
    })
}
```

#### 4.2 Update Response Handler

**File**: `src/streaming/sse.rs`

```rust
impl SSEEventGenerator {
    pub fn generate_events(&mut self, chunk: GeminiStreamChunk) -> Vec<String> {
        let mut events = Vec::new();

        // ... existing token counting ...

        if !self.header_sent {
            events.push(self.format_message_start());
            events.push(self.format_content_block_start());
            self.header_sent = true;
        }

        if let Some(candidate) = chunk.candidates.first() {
            if let Some(content) = &candidate.content {
                for part in &content.parts {
                    match part {
                        // Existing text handling
                        GeminiPart::Text { text } => {
                            if !text.is_empty() {
                                events.push(self.format_content_block_delta(text));
                            }
                        }
                        GeminiPart::TextWithThought { text, .. } => {
                            if !text.is_empty() {
                                events.push(self.format_content_block_delta(text));
                            }
                        }

                        // NEW: Handle function calls
                        GeminiPart::FunctionCall { function_call } => {
                            let tool_use = transform_function_call(function_call)?;

                            // Store mapping for later
                            if let ContentBlock::ToolUse { ref id, ref name, .. } = tool_use {
                                GLOBAL_STATE.register_tool_use(id.clone(), name.clone());
                            }

                            events.push(self.format_tool_use_block(tool_use));
                        }

                        _ => {}
                    }
                }
            }

            // Handle finish with tool_use stop reason
            if let Some(finish_reason) = &candidate.finish_reason {
                // Check if this was a function call
                let stop_reason = if has_function_calls(&chunk) {
                    "tool_use"
                } else {
                    self.map_finish_reason(finish_reason)
                };

                events.push(self.format_content_block_stop());
                events.push(self.format_message_delta(stop_reason));
                events.push(self.format_message_stop());
            }
        }

        events
    }

    fn format_tool_use_block(&self, tool_use: ContentBlock) -> String {
        // Format as Claude content_block_start event with type=tool_use
        let data = json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": tool_use
        });
        format!("event: content_block_start\ndata: {}\n\n", data)
    }
}
```

### Phase 5: Testing Strategy (Week 3)

#### 5.1 Unit Tests

```rust
// src/transform/tools.rs
#[cfg(test)]
mod tests {
    #[test]
    fn test_tool_schema_transformation() {
        // Test all JSON Schema features
        // - primitives (string, number, boolean)
        // - objects with properties
        // - arrays with items
        // - nested objects
        // - required fields
        // - enums
    }

    #[test]
    fn test_function_call_to_tool_use() {
        // Test bidirectional conversion
    }

    #[test]
    fn test_tool_result_to_function_response() {
        // Test with state lookup
    }

    #[test]
    fn test_multiple_parallel_tools() {
        // Claude supports parallel tool calls
    }
}
```

#### 5.2 Integration Tests

```rust
// tests/tool_calling.rs
#[tokio::test]
async fn test_end_to_end_tool_flow() {
    // 1. Send request with tools
    // 2. Verify Gemini receives correct function declarations
    // 3. Mock Gemini response with function call
    // 4. Verify Claude receives correct tool_use block
    // 5. Send tool_result
    // 6. Verify Gemini receives functionResponse
    // 7. Verify final text response
}

#[tokio::test]
async fn test_todo_write_tool() {
    // Real-world test with TodoWrite
}

#[tokio::test]
async fn test_multiple_tools() {
    // Test with multiple tools defined
}

#[tokio::test]
async fn test_tool_errors() {
    // Test error handling in tool results
}
```

#### 5.3 Manual Testing with Claude Code

```bash
# Start proxy
RUST_LOG=debug cargo run

# In another terminal
export ANTHROPIC_BASE_URL=http://localhost:8111
export ANTHROPIC_AUTH_TOKEN="your-gemini-key"
export ANTHROPIC_MODEL=gemini-3-pro-preview

# Test TodoWrite
claude-code
> review the code carefully and update readme

# Should now work without MALFORMED_FUNCTION_CALL!
```

### Phase 6: Error Handling & Edge Cases (Week 3-4)

#### 6.1 Error Scenarios

1. **Unknown tool requested by Gemini**
   - Log warning
   - Return error in functionResponse
   - Continue conversation

2. **Invalid function call arguments**
   - Validate against schema
   - Return structured error to Gemini
   - Allow retry

3. **Missing tool_use_id mapping**
   - Fallback: use tool name from result
   - Log warning for debugging

4. **Schema validation failures**
   - Strict mode: reject request
   - Permissive mode: pass through with warning

#### 6.2 Configuration

```rust
// src/config.rs
pub struct ProxyConfig {
    // ... existing fields ...

    /// Validate tool schemas strictly
    pub strict_tool_validation: bool,

    /// Maximum conversation state retention (minutes)
    pub state_retention_minutes: u64,

    /// Enable tool calling (feature flag)
    pub enable_tool_calling: bool,
}
```

### Phase 7: Performance Optimization (Week 4)

#### 7.1 Schema Caching

```rust
use std::sync::Arc;
use arc_swap::ArcSwap;

/// Cache transformed tool schemas to avoid repeated conversion
pub struct ToolSchemaCache {
    cache: ArcSwap<HashMap<String, GeminiFunctionDeclaration>>,
}

impl ToolSchemaCache {
    pub fn get_or_transform(&self, tool: &ClaudeTool) -> Result<GeminiFunctionDeclaration> {
        // Check cache first
        if let Some(cached) = self.cache.load().get(&tool.name) {
            return Ok(cached.clone());
        }

        // Transform and cache
        let transformed = transform_tool(tool.clone())?;
        self.cache.rcu(|cache| {
            let mut new_cache = (**cache).clone();
            new_cache.insert(tool.name.clone(), transformed.clone());
            new_cache
        });

        Ok(transformed)
    }
}
```

#### 7.2 Metrics

```rust
// Track tool calling performance
pub struct ToolMetrics {
    pub total_tool_calls: AtomicU64,
    pub successful_calls: AtomicU64,
    pub failed_calls: AtomicU64,
    pub avg_latency_ms: AtomicU64,
}
```

## Migration Path

### Stage 1: Feature Flag (Week 1)

```rust
// Default: disabled for safety
pub const ENABLE_TOOL_CALLING: bool = false;

// Enable via env var
if env::var("PROXY_ENABLE_TOOLS").is_ok() {
    // Enable tool transformation
}
```

### Stage 2: Beta Testing (Week 2-3)

- Internal testing with all Claude Code tools
- Monitor logs for edge cases
- Collect metrics

### Stage 3: Production Rollout (Week 4)

- Enable by default
- Update documentation
- Announce feature

## Testing Matrix

| Tool | Test Status | Notes |
|------|-------------|-------|
| TodoWrite | ⏳ Pending | Most common, test first |
| Task | ⏳ Pending | Complex, has subagent_type param |
| Bash | ⏳ Pending | Simple string command |
| Read | ⏳ Pending | File path parameter |
| Edit | ⏳ Pending | Multi-param (file, old, new) |
| Write | ⏳ Pending | File + content |
| Glob | ⏳ Pending | Pattern matching |
| Grep | ⏳ Pending | Multiple params |
| AskUserQuestion | ⏳ Pending | Complex nested structure |
| WebFetch | ⏳ Pending | URL + prompt |
| WebSearch | ⏳ Pending | Query parameter |

## Success Metrics

1. **Functional**
   - ✅ All Claude Code tools work through proxy
   - ✅ Zero MALFORMED_FUNCTION_CALL errors
   - ✅ Multi-turn tool conversations work

2. **Performance**
   - Tool call latency < 50ms overhead
   - Schema transformation cached effectively
   - Memory usage stable across conversations

3. **Reliability**
   - 99.9% tool call success rate
   - Graceful degradation on errors
   - State management robust

## Risks & Mitigation

| Risk | Impact | Mitigation |
|------|--------|------------|
| Schema incompatibility | High | Extensive testing of all JSON Schema features |
| State management bugs | High | Use battle-tested DashMap, add comprehensive tests |
| Gemini API changes | Medium | Version detection, fallback to basic mode |
| Performance degradation | Medium | Caching, metrics, profiling |
| Memory leaks in state | Low | Time-based cleanup, monitoring |

## Open Questions

1. **Session Management**: How to handle multiple concurrent Claude Code sessions?
   - Option A: Use request headers to identify sessions
   - Option B: Use conversation hashing
   - **Decision**: TBD based on Claude Code behavior

2. **Tool Result Streaming**: Can tool execution results be streamed?
   - **Research**: Check Claude Code behavior
   - **Decision**: Start with non-streaming, add later if needed

3. **Rate Limiting**: Should tool calls count toward rate limits differently?
   - **Decision**: Treat same as regular requests

## Future Enhancements

1. **Tool Call Caching**: Cache tool results for deterministic tools
2. **Parallel Tool Execution**: Execute multiple tool calls concurrently
3. **Tool Call Analytics**: Dashboard showing tool usage patterns
4. **Custom Tool Registry**: Allow users to define custom tools
5. **Tool Call Replay**: Record and replay tool calls for debugging

## References

- [Claude Tool Use Documentation](https://docs.claude.com/en/docs/build-with-claude/tool-use)
- [Gemini Function Calling](https://ai.google.dev/gemini-api/docs/function-calling)
- [JSON Schema Specification](https://json-schema.org/specification)
- [Proxy Issue #1: Tool Calling Support](https://github.com/yourproject/issues/1)

## Appendix A: Complete Example

### Input: Claude Request with Tools

```json
{
  "model": "claude-3-5-sonnet-20241022",
  "messages": [
    {"role": "user", "content": "Add a todo to review the design doc"}
  ],
  "tools": [
    {
      "name": "TodoWrite",
      "description": "Create and manage task lists",
      "input_schema": {
        "type": "object",
        "properties": {
          "todos": {
            "type": "array",
            "items": {
              "type": "object",
              "properties": {
                "content": {"type": "string"},
                "status": {"type": "string"},
                "activeForm": {"type": "string"}
              },
              "required": ["content", "status", "activeForm"]
            }
          }
        },
        "required": ["todos"]
      }
    }
  ],
  "max_tokens": 4096
}
```

### Output: Gemini Request with Functions

```json
{
  "contents": [
    {
      "role": "user",
      "parts": [{"text": "Add a todo to review the design doc"}]
    }
  ],
  "tools": [
    {
      "functionDeclarations": [
        {
          "name": "TodoWrite",
          "description": "Create and manage task lists",
          "parameters": {
            "type": "object",
            "properties": {
              "todos": {
                "type": "array",
                "items": {
                  "type": "object",
                  "properties": {
                    "content": {"type": "string"},
                    "status": {"type": "string"},
                    "activeForm": {"type": "string"}
                  },
                  "required": ["content", "status", "activeForm"]
                }
              }
            },
            "required": ["todos"]
          }
        }
      ]
    }
  ],
  "generationConfig": {
    "maxOutputTokens": 4096
  }
}
```

### Gemini Response with Function Call

```json
{
  "candidates": [{
    "content": {
      "parts": [{
        "functionCall": {
          "name": "TodoWrite",
          "args": {
            "todos": [{
              "content": "Review the design doc",
              "status": "pending",
              "activeForm": "Reviewing the design doc"
            }]
          }
        }
      }],
      "role": "model"
    },
    "finishReason": "STOP"
  }]
}
```

### Claude Response with Tool Use

```json
{
  "type": "message",
  "role": "assistant",
  "content": [
    {
      "type": "tool_use",
      "id": "toolu_01DZvF9MqZGT8PqKPmNvwFpK",
      "name": "TodoWrite",
      "input": {
        "todos": [{
          "content": "Review the design doc",
          "status": "pending",
          "activeForm": "Reviewing the design doc"
        }]
      }
    }
  ],
  "stop_reason": "tool_use"
}
```

## Changelog

- 2025-11-19: Initial draft
- TBD: Implementation complete
- TBD: Beta testing complete
- TBD: Production release
