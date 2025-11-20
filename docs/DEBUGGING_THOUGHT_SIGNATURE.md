# Debugging Thought Signature Issue

## Current Status

Tool calling implementation is complete but encountering:
```
Function call is missing a thought_signature in functionCall parts
```

## Root Cause Analysis

The error occurs at **"position 2"** which means the **assistant's message** in the conversation history.

### The Flow

1. **Turn 1**: User asks question
   - Gemini receives tools
   - Gemini calls TodoWrite WITH thoughtSignature
   - Proxy captures signature, stores in state
   - Transforms to tool_use for Claude Code

2. **Turn 2**: Claude Code executes tool, sends result
   - **Assistant message** contains the tool_use block (from Turn 1 response)
   - Proxy must transform tool_use → functionCall WITH thoughtSignature
   - **Problem**: Lookup failing or signature is None

### Possible Issues

**Issue A**: State lookup failing
- Tool_use_id not found in state
- Different ID used?

**Issue B**: Signature was never captured
- First call didn't have signature?
- Not properly extracted from Gemini response?

**Issue C**: State cleared between turns
- State persistence issue?
- Race condition?

## Debugging Steps

### 1. Restart Proxy with Debug Logging

```bash
# Kill current proxy
# Then start with:
RUST_LOG=debug cargo run --release
```

### 2. Start Fresh Claude Code

```bash
# Exit current Claude Code session
# Start new:
claude-code
```

### 3. Try Simple Command

```
> What's 2+2?
```

Watch proxy logs for:
```
DEBUG ... Looking up thought signature for tool_use
DEBUG ... found_metadata=true has_signature=true
```

OR

```
WARN ... Tool_use has no thought_signature in state
```

### 4. Check State Registration

When Gemini calls a function, watch for:
```
DEBUG ... Registering tool use mapping
DEBUG ... has_signature=true
```

## Expected vs Actual

### Expected Flow (Working)
```
Gemini response:
  functionCall: { name: "TodoWrite", args: {...}, thoughtSignature: "ABC123" }

Proxy:
  register_tool_use(toolu_XYZ, "TodoWrite", Some("ABC123"))

State:
  toolu_XYZ → { name: "TodoWrite", signature: Some("ABC123") }

Next turn (assistant message with tool_use):
  tool_use: { id: "toolu_XYZ", ... }
  get_metadata("toolu_XYZ") → Some({ signature: Some("ABC123") })

  Send back:
  functionCall: { name: "TodoWrite", thoughtSignature: "ABC123" } ✅
```

### Actual Flow (Broken)
```
Gemini response:
  functionCall: { name: "TodoWrite", ... } (signature missing or not captured?)

Proxy:
  register_tool_use(toolu_XYZ, "TodoWrite", None)

State:
  toolu_XYZ → { name: "TodoWrite", signature: None }

Next turn:
  get_metadata("toolu_XYZ") → Some({ signature: None })

  Send back:
  functionCall: { name: "TodoWrite" } (no signature) ❌
  Gemini: 400 Bad Request
```

## Investigation Checklist

- [ ] Verify Gemini is sending thoughtSignature in first function call
- [ ] Verify SSEEventGenerator is capturing it (`function_call.thought_signature`)
- [ ] Verify state registration includes signature (`register_tool_use` with 3 params)
- [ ] Verify state lookup finds the entry (`get_metadata` returns Some)
- [ ] Verify signature is not None in the metadata
- [ ] Verify it's included in the FunctionCall struct

## Potential Fixes

### Fix 1: Gemini Doesn't Send Signatures

If Gemini 3 Pro isn't sending signatures in the FIRST function call:

**Solution**: We can't get what doesn't exist. May need to:
- Use a different Gemini model (2.0 Flash instead of 3 Pro)
- Generate fake signatures
- Disable tools temporarily

### Fix 2: Signature Not Being Captured

If Gemini sends but we don't capture:

**Check**: `src/streaming/sse.rs:80`
```rust
function_call.thought_signature.clone()
```

Should be extracting the signature.

### Fix 3: Wrong Tool_use_id

If we're using different IDs:

**Check**: Are we generating a new ID or using Gemini's?
- We generate: `toolu_{uuid}`
- Need to use the SAME id consistently

## Next Steps

1. Run proxy with `RUST_LOG=debug`
2. Check logs for signature capture and lookup
3. Identify which of Issues A/B/C is the problem
4. Apply appropriate fix
5. Test again

## Alternative: Gemini 2.0 Flash

If Gemini 3 Pro is too strict with thought signatures, consider using:

```rust
pub fn map_model_name(_claude_model: &str) -> &'static str {
    "gemini-2.0-flash-exp"  // Doesn't require thought signatures
}
```

This would work but lose Gemini 3 Pro's advanced reasoning capabilities.
