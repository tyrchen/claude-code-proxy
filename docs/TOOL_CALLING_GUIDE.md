# Tool Calling Manual Testing Guide

This guide explains how to manually test the tool calling feature with Claude Code.

## Prerequisites

1. **Gemini API Key**: Get one from https://aistudio.google.com/apikey
2. **Claude Code**: Install from https://github.com/anthropics/claude-code
3. **Proxy built**: Run `cargo build --release`

## Setup

### 1. Start the Proxy with Debug Logging

```bash
export GEMINI_API_KEY="your-gemini-api-key"
RUST_LOG=debug cargo run --release
```

You should see:
```
INFO  claude_code_proxy: Starting Claude-to-Gemini proxy...
INFO  claude_code_proxy:   Listen: 127.0.0.1:8111
INFO  claude_code_proxy: Proxy ready!
```

### 2. Configure Claude Code

In a new terminal:

```bash
export ANTHROPIC_BASE_URL=http://localhost:8111
export ANTHROPIC_AUTH_TOKEN="your-gemini-api-key"
export ANTHROPIC_MODEL=gemini-3-pro-preview

# Optional: Set specific models
export ANTHROPIC_DEFAULT_SONNET_MODEL=gemini-3-pro-preview
export CLAUDE_CODE_SUBAGENT_MODEL=gemini-3-pro-preview

# Start Claude Code
claude-code
```

## Test Cases

### Test 1: TodoWrite Tool (Critical - This was failing before)

**Objective**: Verify Claude Code can use TodoWrite to track tasks

**Steps**:
1. In Claude Code, type: `review the code carefully and update readme`
2. Watch the proxy logs for:
   ```
   DEBUG claude_code_proxy::transform::tools: Transforming tool: TodoWrite
   INFO  claude_code_proxy::handler: SSE: event: content_block_start (tool_use)
   ```
3. Claude Code should display a todo list
4. The conversation should continue normally

**Expected**:
- ✅ No `MALFORMED_FUNCTION_CALL` errors
- ✅ Todo list appears in Claude Code
- ✅ Claude Code continues working on tasks

**Verify in logs** (`/tmp/gemini.log`):
```json
{
  "tools": [{
    "functionDeclarations": [{
      "name": "TodoWrite",
      "parameters": {
        "type": "object",
        "properties": {
          "todos": { "type": "array" }
        }
      }
    }]
  }]
}
```

### Test 2: Task Tool (Subagent Spawning)

**Objective**: Test complex tool with subagent_type parameter

**Steps**:
1. Ask: `Find all error handling code in the codebase`
2. Claude Code should spawn an Explore agent
3. Watch for Task tool call in logs

**Expected**:
- ✅ Task tool transformed correctly
- ✅ Subagent spawns successfully
- ✅ Results returned

### Test 3: Bash Tool (Simple Parameter)

**Objective**: Test simple string parameter tool

**Steps**:
1. Ask: `What's in the current directory?`
2. Claude Code should use Bash tool: `ls`
3. Watch for function call in logs

**Expected**:
- ✅ Bash command executes
- ✅ Output displayed
- ✅ Conversation continues

### Test 4: Read Tool (File Operations)

**Objective**: Test file path parameter

**Steps**:
1. Ask: `Show me the README file`
2. Claude Code uses Read tool
3. File content displayed

**Expected**:
- ✅ File read successfully
- ✅ Content displayed
- ✅ No errors

### Test 5: AskUserQuestion Tool (Complex Nested)

**Objective**: Test complex nested schema

**Steps**:
1. Ask: `I need to refactor this codebase, what approach should we use?`
2. Claude Code presents multiple options
3. You select an option

**Expected**:
- ✅ Question schema transforms correctly
- ✅ Options displayed
- ✅ Selection works

### Test 6: Multi-Turn Tool Conversation

**Objective**: Test state persistence across turns

**Steps**:
1. Ask: `Create a new Rust module for authentication`
2. Claude Code uses TodoWrite
3. Then uses Write to create files
4. Then uses Bash to run tests
5. Each tool call is a separate turn

**Expected**:
- ✅ All tool calls work
- ✅ State persists (tool_use_ids → function_names)
- ✅ No lookup failures in logs

### Test 7: Parallel Tool Calls

**Objective**: Test multiple tools in one response

**Steps**:
1. Ask: `Show me README.md and run cargo test`
2. Claude Code might use Read and Bash in parallel

**Expected**:
- ✅ Both tools execute
- ✅ Results returned correctly
- ✅ No race conditions

### Test 8: Tool Error Handling

**Objective**: Test error propagation

**Steps**:
1. Ask: `Read a file that doesn't exist: /nonexistent/file.txt`
2. Claude Code uses Read tool
3. Tool returns error

**Expected**:
- ✅ Error properly formatted
- ✅ Claude Code handles error gracefully
- ✅ Conversation continues

## Debugging

### Check Proxy Logs

```bash
# In proxy terminal, watch for:
tail -f /tmp/gemini.log | grep -E "(REQUEST|RESPONSE|SSE EVENT)"
```

### Enable Extra Logging

```bash
RUST_LOG=claude_code_proxy=trace cargo run --release
```

### Verify Tool Transformation

After each request, check `/tmp/gemini.log` for:

```json
=== REQUEST ===
{
  "tools": [
    {
      "functionDeclarations": [
        {
          "name": "ToolName",
          "description": "...",
          "parameters": { ... }
        }
      ]
    }
  ]
}
```

### Verify Function Calls

Check for Gemini responses with:

```json
=== RESPONSE CHUNK ===
{
  "candidates": [{
    "content": {
      "parts": [{
        "functionCall": {
          "name": "ToolName",
          "args": { ... }
        }
      }]
    }
  }]
}
```

### Verify SSE Events

Check for Claude-formatted events:

```
=== SSE EVENT ===
event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_...","name":"ToolName","input":{...}}}
```

## Troubleshooting

### Problem: MALFORMED_FUNCTION_CALL

**Symptoms**: Gemini returns `finishReason: "MALFORMED_FUNCTION_CALL"`

**Cause**: Tool schema not properly transformed

**Solution**:
1. Check `/tmp/gemini.log` - verify `tools` field exists in request
2. Verify schema has correct `functionDeclarations` structure
3. Check for schema validation errors in proxy logs

### Problem: State Lookup Failures

**Symptoms**: Logs show "No function name found in state"

**Cause**: tool_use_id not registered when function call received

**Solution**:
1. Verify SSEEventGenerator is using GLOBAL_STATE
2. Check `register_tool_use()` is called in streaming handler
3. Check state isn't being cleared between turns

### Problem: Tool Use Not Appearing

**Symptoms**: Claude Code doesn't show tool execution

**Cause**: SSE events not properly formatted

**Solution**:
1. Check content_block_start event has type="tool_use"
2. Verify stop_reason is "tool_use" not "end_turn"
3. Check SSE event format matches Claude's spec exactly

## Metrics

Check metrics during operation:

```bash
# In Rust code or tests:
use claude_code_proxy::metrics::TOOL_METRICS;

let snapshot = TOOL_METRICS.snapshot();
println!("{}", snapshot);
```

Expected output:
```
Tool Metrics: 42 calls (98.5% success), 40 results, 1 state failures, avg 0.15ms
```

## Performance Benchmarks

Expected latency overhead:
- Tool schema transformation: < 0.1ms (cached)
- Function call transformation: < 0.05ms
- State lookup: < 0.01ms
- Total overhead: < 0.2ms per tool call

## Success Criteria

After testing, verify:

- [ ] All 11 Claude Code tools work (TodoWrite, Task, Bash, Read, Edit, Write, Glob, Grep, AskUserQuestion, WebFetch, WebSearch)
- [ ] No MALFORMED_FUNCTION_CALL errors
- [ ] Multi-turn conversations work
- [ ] Parallel tool calls work
- [ ] Error handling works
- [ ] State persists across turns
- [ ] Cache reduces latency
- [ ] Metrics show 99%+ success rate

## Known Limitations

1. **Global State**: Current implementation uses a global singleton. For multi-user deployments, implement per-session state.

2. **State Cleanup**: Automatic cleanup runs every hour. For high-traffic deployments, implement more aggressive cleanup.

3. **Cache Size**: Cache grows unbounded. For production, implement LRU eviction.

## Next Steps

If testing reveals issues:

1. Check error logs in proxy terminal
2. Examine `/tmp/gemini.log` for request/response details
3. Enable RUST_LOG=trace for verbose debugging
4. Report issues with full logs

## Example Session

```
You: review the code carefully and update readme

Claude Code: I'll help you review the code and update the README.
[Uses TodoWrite tool to create tasks]
✅ Added tasks:
  1. Review codebase architecture
  2. Check for code quality issues
  3. Update README documentation

[Continues working...]
```

This should work seamlessly without any MALFORMED_FUNCTION_CALL errors!
