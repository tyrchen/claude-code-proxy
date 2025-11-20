# Spec 0006: Smart Todo Completion Handling (Revised)

## Problem Statement

When using Gemini with Claude Code's TodoWrite tool, Gemini gets stuck in infinite loops because:

1. Gemini calls `TodoWrite` to create todos (e.g., "Run cargo clippy - status: in_progress")
2. Gemini calls `Bash` to execute the command in parallel
3. Bash completes successfully and returns results
4. TodoWrite returns "Todos have been modified successfully"
5. **But the todo items remain in "in_progress" state** - they're never marked as "completed"
6. Gemini sees incomplete todos and calls the same tools again → infinite loop

**Root Cause**: Claude Code's TodoWrite system expects Claude's reasoning to update todo status after task completion. Gemini doesn't make this connection automatically.

## Design Goals

1. **Transparent**: Don't break existing Claude Code behavior
2. **Smart**: Automatically mark todos as completed when their corresponding tools succeed
3. **Minimal**: Only intervene when necessary (TodoWrite + tool execution pattern)
4. **Stateful**: Track which todos correspond to which tool executions
5. **Non-intrusive**: The proxy should help, not control the conversation

## Proposed Solution (REVISED - Simpler & Better)

### Core Idea

Instead of the proxy guessing which todos to complete, **inject a reminder prompt after successful tool executions** asking Gemini to update the TodoWrite list.

### Architecture

Simple prompt injection in request transformation:
1. Detect when tool results are being sent back (user message with functionResponse)
2. Check if there are any non-TodoWrite tool results that succeeded
3. Inject a text part reminding Gemini to update todos if needed
4. Let Gemini decide what to mark as completed

### Data Flow

```
Gemini Response:
├─ TodoWrite({todos: [{content: "Run clippy", status: "in_progress"}]})
└─ Bash({command: "cargo clippy"})
    ↓
Claude Code executes and returns results:
├─ TodoWrite result: "Success"
└─ Bash result: "Finished successfully" (error: false)
    ↓
Proxy modifies user message before sending to Gemini:
├─ Original: User[functionResponse(TodoWrite), functionResponse(Bash)]
├─ Modified: User[
│     functionResponse(Bash),  // Keep Bash result
│     text("Based on the tool results above, update your TodoWrite list to mark completed tasks as 'completed' or add new tasks as needed.")
│   ]
    ↓
Gemini sees:
├─ Tool succeeded
├─ Reminder to update todos
└─ Calls TodoWrite to mark task as completed
```

### Implementation Plan

#### Phase 1: Todo Tracking (Core)

**1.1 Todo-Tool Mapping State**
- Extend `ToolCallMetadata` to include `related_todo_content: Option<String>`
- When TodoWrite creates todo with status "in_progress"/"pending", extract the content
- When subsequent tool (Bash/Read/etc) is called, associate it with the active todo
- Use heuristics: match todo content keywords with tool parameters

**1.2 Matching Logic**
```rust
struct TodoManager {
    // Maps todo content → tool_use_id
    active_todos: DashMap<String, Vec<String>>,

    // Maps tool_use_id → todo content it's working on
    tool_to_todo: DashMap<String, String>,
}

impl TodoManager {
    fn extract_todos_from_call(args: &Value) -> Vec<(String, String)> {
        // Extract (content, status) pairs from TodoWrite args
    }

    fn match_tool_to_todo(tool_name: &str, tool_args: &Value, active_todos: &[(String, String)]) -> Option<String> {
        // Heuristic matching:
        // - Bash command contains todo keywords
        // - Read/Edit file path mentioned in todo
        // - Sequential ordering (first pending todo → next tool call)
    }
}
```

#### Phase 2: Completion Detection

**2.1 Monitor Tool Results**
- In `extract_parts` when processing `ToolResult`
- Check if `is_error == false` (success)
- Lookup if this tool_use_id has associated todo
- If yes, mark for completion

**2.2 Generate Synthetic TodoWrite**
```rust
fn generate_todo_completion(
    original_todos: Vec<TodoItem>,
    completed_content: &str
) -> ContentBlock {
    // Clone the original todo list
    // Find the todo with matching content
    // Update its status to "completed"
    // Generate a new TodoWrite ToolResult block
}
```

#### Phase 3: Injection Strategy

**Option A: Modify Response Stream**
- When Gemini calls TodoWrite + another tool in parallel
- Store the original TodoWrite args
- After the other tool completes, inject a synthetic TodoWrite completion

**Option B: Modify Request**
- When sending tool results back to Gemini
- If a tool succeeded and had an associated todo
- Add a synthetic functionResponse for TodoWrite with updated status

**Recommendation**: **Option B** (modify request) is cleaner:
- Less complex streaming logic
- Easier to test
- More predictable behavior

### Implementation Details

#### 3.1 State Structure
```rust
pub struct TodoTracker {
    // Current active todos from latest TodoWrite call
    active_todos: Arc<DashMap<String, TodoStatus>>,

    // Map tool_use_id → todo content
    tool_todo_map: Arc<DashMap<String, String>>,

    // Latest TodoWrite call args (for generating updates)
    last_todowrite_args: Arc<RwLock<Option<Value>>>,
}

struct TodoStatus {
    content: String,
    status: String,  // "pending", "in_progress", "completed"
    active_form: String,
}
```

#### 3.2 Hook Points

**Hook 1: Response Processing** (`streaming/sse.rs`)
```rust
GeminiPart::FunctionCallWithThought { function_call, .. } => {
    if function_call.name == "TodoWrite" {
        // Extract and store todos
        todo_tracker.update_todos(&function_call.args);
    } else {
        // Try to match with active todo
        if let Some(todo) = todo_tracker.match_tool_to_todo(&function_call.name, &function_call.args) {
            // Store mapping
            // (will use tool_use_id after transform)
        }
    }
    // ... rest of transform logic
}
```

**Hook 2: Request Processing** (`transform/request.rs`)
```rust
ContentBlock::ToolResult { tool_use_id, is_error, .. } => {
    let function_name = state.get_function_name(&tool_use_id);

    // If tool succeeded and has associated todo
    if !is_error.unwrap_or(false)
        && function_name != "TodoWrite"
        && let Some(todo) = todo_tracker.get_todo_for_tool(&tool_use_id)
    {
        // Generate synthetic TodoWrite completion
        let completion = todo_tracker.generate_completion(&todo);
        parts.push(completion);
    }

    // Add original tool result
    parts.push(GeminiPart::FunctionResponse { ... });
}
```

### Edge Cases

1. **Multiple todos in one call**: Match based on order or keywords
2. **Todo without tool**: Don't intervene
3. **Tool without todo**: Normal behavior
4. **Nested/dependent todos**: Only mark completed the one that matches
5. **Tool fails**: Don't mark todo as completed
6. **TodoWrite called multiple times**: Replace active todo list

### Testing Strategy

```rust
#[test]
fn test_todo_completion_basic() {
    // 1. Gemini calls TodoWrite + Bash in parallel
    // 2. Bash succeeds
    // 3. Verify synthetic TodoWrite completion is generated
    // 4. Verify completed status in synthetic response
}

#[test]
fn test_todo_completion_multiple_tools() {
    // TodoWrite with 3 todos, 3 tools called
    // Verify each completion is tracked separately
}

#[test]
fn test_todo_no_match() {
    // TodoWrite without corresponding tool
    // Verify no synthetic completion
}

#[test]
fn test_tool_failure() {
    // Tool fails (error: true)
    // Verify todo NOT marked as completed
}
```

### Configuration

Add optional feature flag:
```rust
pub struct ProxyConfig {
    // ... existing fields

    /// Enable smart todo completion handling
    /// When true, automatically marks todos as completed when tools succeed
    pub smart_todo_completion: bool,
}
```

### Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Wrong todo marked completed | Use conservative matching (exact keyword match) |
| Breaks Claude Code expectations | Make it opt-in via config flag |
| State memory leak | Use same cleanup as ToolCallMetadata (1 hour TTL) |
| Race conditions | Use DashMap for thread-safe access |
| Matching ambiguity | Log all matches at DEBUG level for troubleshooting |

## Implementation Phases

### Phase 1: Foundation (Required for MVP)
- [ ] Add `TodoTracker` struct to state.rs
- [ ] Implement todo extraction from TodoWrite args
- [ ] Add hook in SSE generator to capture TodoWrite calls
- [ ] Basic logging to verify todos are captured

### Phase 2: Matching (Core Logic)
- [ ] Implement tool-to-todo matching heuristics
- [ ] Store mappings in TodoTracker
- [ ] Add tests for matching logic
- [ ] Handle edge cases (multiple todos, no match, etc.)

### Phase 3: Completion Generation (Critical)
- [ ] Implement synthetic TodoWrite response generation
- [ ] Inject completions in request transformation
- [ ] Verify Claude Code accepts synthetic responses
- [ ] Add comprehensive tests

### Phase 4: Polish
- [ ] Add configuration flag
- [ ] Performance optimization
- [ ] Documentation
- [ ] Integration tests with real Claude Code

## Success Criteria

1. ✅ Gemini calls TodoWrite + Bash in parallel
2. ✅ Bash completes successfully
3. ✅ Proxy automatically marks corresponding todo as "completed"
4. ✅ Gemini sees completed todo and moves to next task
5. ✅ No infinite loops
6. ✅ All existing tests still pass

## Alternatives Considered

### Alternative 1: Strip TodoWrite Entirely
**Rejected**: Loses valuable task tracking that helps Gemini organize work

### Alternative 2: Proxy Makes Decisions
**Rejected**: Too invasive - proxy shouldn't decide when work is done

### Alternative 3: Prompt Engineering
**Rejected**: Can't fix this via prompts - it's a structural issue

### Alternative 4: Smart Completion (Chosen)
**Selected**: Balances automation with transparency

## Open Questions

1. Should we match ALL todos or just "in_progress" ones?
   - **Answer**: Only "in_progress" - pending means not started yet

2. What if tool partially succeeds (warnings but no errors)?
   - **Answer**: Mark completed if error=false, regardless of warnings

3. Should we inject completions immediately or batch them?
   - **Answer**: Immediate - one completion per tool success

4. What about Task tool (spawns subagent)?
   - **Answer**: Treat like any other tool - match by keywords

## Timeline

- Phase 1: 1-2 hours
- Phase 2: 2-3 hours
- Phase 3: 2-3 hours
- Phase 4: 1 hour

**Total**: Can be completed in one development session

## References

- Gemini function calling spec: https://ai.google.dev/gemini-api/docs/function-calling
- Claude Code TodoWrite tool behavior
- DashMap documentation for thread-safe state management
