# Kimi Logging Documentation

This document explains the comprehensive logging added for the Kimi provider to help understand how Claude Code interactions work with Kimi's API.

## Log File Location

All Kimi requests and responses are logged to: `/tmp/kimi.log`

## What Gets Logged

### 1. Request Logging

When a request is sent to Kimi, the following information is logged:

```
================================================================================
=== KIMI REQUEST (XXX bytes) ===
================================================================================
URL: https://api.moonshot.ai/anthropic/v1/messages
Model: kimi-k2-thinking-turbo

--- REQUEST BODY (Pretty JSON) ---
{
  "model": "kimi-k2-thinking-turbo",
  "max_tokens": 32000,
  "messages": [
    {
      "role": "user",
      "content": "Hello"
    }
  ],
  "tools": [
    {
      "name": "TodoWrite",
      "description": "...",
      "input_schema": { ... }
    }
  ],
  "stream": true
}
```

**What this shows:**
- Exact URL being called
- Model name being used
- Complete Claude Messages API request in pretty-printed JSON format
- All tools available (including TodoWrite)
- All messages in the conversation
- All parameters (max_tokens, temperature, etc.)

### 2. Response Status Logging

```
--- RESPONSE STATUS ---
Status: 200 OK
```

**What this shows:**
- HTTP status code from Kimi API
- Whether the request succeeded or failed

### 3. Streaming Response Chunks

For each chunk received from Kimi's streaming API:

```
--- STREAMING RESPONSE CHUNK ---
event: message_start
data: {"type":"message_start","message":{"id":"msg_...","model":"kimi-k2-thinking-turbo","role":"assistant","content":[],"stop_reason":null}}

--- STREAMING RESPONSE CHUNK ---
event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

--- STREAMING RESPONSE CHUNK ---
event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}

--- STREAMING RESPONSE CHUNK ---
event: tool_use
data: {"type":"tool_use","id":"toolu_...","name":"TodoWrite","input":{"todos":[{"content":"Task 1","status":"pending","activeForm":"Doing task 1"}]}}

--- STREAMING RESPONSE CHUNK ---
event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null},"usage":{"output_tokens":150}}

--- STREAMING RESPONSE CHUNK ---
event: message_stop
data: {"type":"message_stop"}
```

**What this shows:**
- Raw SSE (Server-Sent Events) format from Kimi
- Each streaming event with its type
- Progressive text generation (text_delta events)
- Tool use calls (when Claude wants to use TodoWrite or other tools)
- Token usage information
- Stop reasons (end_turn, tool_use, etc.)

### 4. Error Logging

If the request fails:

```
--- ERROR RESPONSE ---
{
  "error": {
    "type": "invalid_request_error",
    "message": "API key is invalid"
  }
}
```

**What this shows:**
- Detailed error information from Kimi API
- Error type and message
- Helps debug authentication or request format issues

## How to Use the Logs

### Understanding TodoWrite Flow

1. **Request Phase**: Look for the `REQUEST BODY` section to see:
   - If TodoWrite tool is defined in the `tools` array
   - The full schema that Claude Code provides to Kimi
   - Any previous tool_result messages with TodoWrite results

2. **Response Phase**: Look for `STREAMING RESPONSE CHUNK` sections to see:
   - When Kimi decides to call TodoWrite (look for `event: tool_use`)
   - The exact parameters being passed to TodoWrite
   - The complete todo list structure

3. **Tool Results**: In subsequent requests, look for messages with `tool_result` role:
   ```json
   {
     "role": "user",
     "content": [
       {
         "type": "tool_result",
         "tool_use_id": "toolu_...",
         "content": "Todos have been modified successfully..."
       }
     ]
   }
   ```

### Comparing with Gemini

**Key Differences to Look For:**

1. **Request Format**:
   - Kimi: Pure Claude Messages API format (unchanged)
   - Gemini: Transformed to Gemini's format (see `/tmp/gemini.log`)

2. **Response Format**:
   - Kimi: Claude-compatible SSE events
   - Gemini: Custom JSON chunks that get transformed to SSE

3. **Tool Handling**:
   - Kimi: Native Claude tool_use format
   - Gemini: Transformed to function_call/function_response

### Example Analysis Workflow

```bash
# Clear old logs
rm /tmp/kimi.log

# Start the proxy with Kimi
export ANTHROPIC_AUTH_TOKEN='your-kimi-token'
export ANTHROPIC_MODEL='kimi-k2-thinking-turbo'
claude-code-proxy kimi

# Use Claude Code normally, then analyze the log

# View the entire conversation flow
cat /tmp/kimi.log

# Find all TodoWrite tool uses
grep -A 10 "TodoWrite" /tmp/kimi.log

# See all streaming chunks
grep -A 5 "STREAMING RESPONSE CHUNK" /tmp/kimi.log

# Check for errors
grep -A 10 "ERROR" /tmp/kimi.log
```

## Log Format Design

The logging is designed to be:

1. **Human-Readable**: Pretty-printed JSON, clear section markers
2. **Grep-Friendly**: Consistent section headers for easy filtering
3. **Complete**: Captures every byte sent and received
4. **Sequential**: Maintains temporal order of events
5. **Append-Only**: Never overwrites, making it easy to track multiple requests

## Performance Notes

- Logging adds minimal overhead (file I/O is asynchronous)
- Log file can grow large during heavy usage
- Consider rotating or clearing `/tmp/kimi.log` periodically
- Logs contain sensitive data (API keys are NOT logged, but conversation content is)

## Privacy Considerations

⚠️ **Important**: The log files contain:
- Complete conversation content
- Tool definitions and parameters
- User queries and assistant responses
- Do NOT share log files publicly without sanitization

## Troubleshooting with Logs

### Issue: TodoWrite not being called

**Check**:
1. Request body has TodoWrite in the tools array
2. Response chunks show Kimi considered it (may not use it)
3. Compare with working Gemini example

### Issue: TodoWrite parameters incorrect

**Check**:
1. The tool_use event in streaming chunks
2. The input_schema in the request
3. Compare actual parameters vs schema

### Issue: Tool results not working

**Check**:
1. Subsequent request has tool_result with correct tool_use_id
2. Content format matches Kimi's expectations
3. Response acknowledges the tool result

## Future Enhancements

Potential improvements to logging:

- [ ] Structured logging (JSON format for easier parsing)
- [ ] Request/response correlation IDs
- [ ] Performance metrics (latency, token counts)
- [ ] Log rotation based on size
- [ ] Redaction of sensitive content
- [ ] Separate log levels (debug, info, error)
