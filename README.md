# Claude Code Proxy

A high-performance protocol translation proxy that allows Claude Code CLI to use Google Gemini models transparently.

## Status: Phase 1 & 2 Complete ✅

**Current Implementation:**
- ✅ Complete data models for Claude and Gemini APIs
- ✅ Full request transformation pipeline (Claude → Gemini)
- ✅ Comprehensive validation
- ✅ Configuration system
- ✅ 35/35 tests passing

**In Progress:**
- ⏳ Streaming response parser (Phase 3)
- ⏳ SSE event generation (Phase 3)
- ⏳ Pingora integration (Phase 4)

## Quick Start

### Build

```bash
cargo build --release
```

### Run Tests

```bash
# All tests
cargo test

# Just library tests
cargo test --lib

# Just integration tests
cargo test --test request_transform

# With output
cargo test -- --nocapture
```

### Test Results

```
running 35 tests
test result: ok. 35 passed; 0 failed; 0 ignored
```

## Architecture

```
┌─────────────────┐
│   Claude Code   │
│   CLI Client    │
└────────┬────────┘
         │ POST /v1/messages
         │ Claude Messages API
         │
         ▼
┌─────────────────────────────────────────┐
│      Pingora Proxy (Phase 4)            │
│  ┌───────────────────────────────────┐  │
│  │  Request Pipeline ✅ DONE          │  │
│  │  1. Parse Claude request          │  │
│  │  2. Validate request              │  │
│  │  3. Map model names               │  │
│  │  4. Transform to Gemini format    │  │
│  └───────────────────────────────────┘  │
│  ┌───────────────────────────────────┐  │
│  │  Response Pipeline ⏳ PHASE 3     │  │
│  │  1. Parse Gemini JSON stream      │  │
│  │  2. Generate SSE events           │  │
│  │  3. Stream to client              │  │
│  └───────────────────────────────────┘  │
└────────┬────────────────────────────────┘
         │ POST /v1beta/models/{model}:streamGenerateContent
         │ Gemini API
         │
         ▼
┌─────────────────┐
│  Google Gemini  │
│      API        │
└─────────────────┘
```

## Features Implemented (Phase 1-2)

### Data Models
- **Claude Types**: Request, Message, Content blocks, SSE events
- **Gemini Types**: Request, Content, Response chunks, Usage metadata
- Full serde support with proper JSON formatting

### Request Transformation
- **Model Mapping**: Automatic Claude → Gemini model selection
  - `claude-opus` → `gemini-1.5-pro`
  - `claude-sonnet` → `gemini-2.0-flash-exp`
  - `claude-haiku` → `gemini-2.0-flash-exp`
- **Role Mapping**: `assistant` → `model`, `user` → `user`
- **System Prompts**: Converts to Gemini's systemInstruction format
- **Generation Config**: Maps all parameters (temperature, top_p, top_k, max_tokens)

### Validation
Comprehensive request validation:
- Message structure checks
- Role alternation enforcement
- Parameter range validation
- Detailed error messages

### Configuration
- Environment variable support
- TOML file configuration
- Validation on load

## Example Usage

```rust
use claude_code_proxy::*;
use std::fs;

// Load test fixture
let json = fs::read_to_string("tests/fixtures/claude_request_simple.json")?;
let claude_req: ClaudeRequest = serde_json::from_str(&json)?;

// Validate
validate_claude_request(&claude_req)?;

// Map model name
let target_model = map_model_name(&claude_req.model);
// "claude-3-5-sonnet-20241022" → "gemini-2.0-flash-exp"

// Transform to Gemini format
let gemini_req = transform_request(claude_req)?;

// Serialize
let gemini_json = serde_json::to_string_pretty(&gemini_req)?;
println!("{}", gemini_json);
```

Output:
```json
{
  "contents": [
    {
      "role": "user",
      "parts": [
        {
          "text": "Hello, how are you?"
        }
      ]
    }
  ],
  "generationConfig": {
    "maxOutputTokens": 100
  }
}
```

## Project Structure

```
claude-code-proxy/
├── src/
│   ├── lib.rs                    # Public API
│   ├── error.rs                  # Error types
│   ├── config.rs                 # Configuration
│   ├── models/
│   │   ├── claude.rs             # Claude API types (15 types)
│   │   └── gemini.rs             # Gemini API types (12 types)
│   ├── transform/
│   │   ├── request.rs            # Request transformation
│   │   └── validation.rs         # Validation logic
│   └── streaming/                # Phase 3 (in progress)
│
├── tests/
│   ├── fixtures/                 # Test JSON files
│   └── request_transform.rs     # Integration tests
│
├── specs/
│   ├── 0001-spec.md             # Original specification
│   ├── 0002-design-spec.md      # Detailed design
│   └── 0003-plan.md             # Implementation plan
│
├── IMPLEMENTATION_STATUS.md     # Current status
└── Cargo.toml
```

## Testing

### Test Coverage

- **Unit Tests**: 29 tests covering all core logic
- **Integration Tests**: 6 end-to-end transformation tests
- **Test Fixtures**: 5 realistic JSON examples

### Run Specific Tests

```bash
# Model mapping
cargo test test_model_mapping

# Request transformation
cargo test transform_request

# Validation
cargo test validate

# Integration
cargo test --test request_transform
```

## Dependencies

```toml
[dependencies]
anyhow = "1.0.100"
async-trait = "0.1"
bytes = "1.6"
pingora = { version = "0.6.0", features = ["cache", "lb", "rustls"] }
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.145"
thiserror = "2.0.17"
tokio = { version = "1.48.0", features = ["rt-multi-thread", "macros"] }
toml = "0.8"
uuid = { version = "1.11", features = ["v4", "serde"] }
```

## Development

### Running Tests with Coverage

```bash
cargo test --verbose
```

### Code Quality

```bash
# Format
cargo fmt

# Lint
cargo clippy

# Check compilation
cargo check
```

## Roadmap

### Phase 3: Response Pipeline (Next)
- [ ] Streaming JSON parser with state machine
- [ ] SSE event generator
- [ ] Buffer management
- [ ] Integration tests

### Phase 4: Pingora Integration
- [ ] Implement ProxyHttp trait
- [ ] Request/response filters
- [ ] Upstream peer configuration
- [ ] Error handling

### Phase 5: Testing & Refinement
- [ ] Load testing
- [ ] Performance benchmarks
- [ ] Error scenario testing
- [ ] Edge case handling

### Phase 6: Documentation & Release
- [ ] API documentation
- [ ] Usage examples
- [ ] Deployment guide
- [ ] CI/CD setup

## Configuration

### Environment Variables

```bash
export GEMINI_API_KEY="your-api-key-here"
export PROXY_LISTEN_ADDR="127.0.0.1:8080"
export PROXY_WORKERS="4"
export GEMINI_ENDPOINT="generativelanguage.googleapis.com"
```

### TOML Configuration (Future)

```toml
[server]
listen_addr = "127.0.0.1:8080"
workers = 4

[gemini]
api_key = "${GEMINI_API_KEY}"
endpoint = "generativelanguage.googleapis.com"
```

## License

MIT

## Contributing

This project follows the implementation plan in `specs/0003-plan.md`. See `IMPLEMENTATION_STATUS.md` for current progress.

## Documentation

- **Specification**: `specs/0001-spec.md` - Original research and requirements
- **Design**: `specs/0002-design-spec.md` - Detailed architecture and design
- **Plan**: `specs/0003-plan.md` - 18-day implementation roadmap
- **Status**: `IMPLEMENTATION_STATUS.md` - Current progress report

## Performance Goals

- **Latency Overhead**: < 5ms
- **Throughput**: > 1000 req/s
- **Memory per Request**: < 1MB
- **Transformation**: < 1μs

## Support

For issues or questions, see the implementation plan and design documentation in the `specs/` directory.
