# Claude-to-Gemini Proxy: Axum + Reqwest Redesign

## Problem Statement

Pingora's proxy architecture has a fundamental limitation: it cannot transform request bodies that require knowing the final size before sending headers. Specifically:

1. Gemini's `streamGenerateContent` endpoint REQUIRES `Content-Length` header (not chunked encoding)
2. To set `Content-Length`, we must know the transformed body size
3. To know the size, we must transform BEFORE sending headers
4. But transforming requires reading the full body in `request_filter`
5. Reading body in `request_filter` prevents it from being sent to upstream (Pingora issue #349)

**Conclusion**: Pingora's ProxyHttp trait cannot support this use case.

## Solution: Axum + Reqwest

Use axum as the HTTP server and reqwest as the HTTP client to have full control over the request/response lifecycle.

### Architecture

```
Claude Code → Axum Server → Reqwest Client → Gemini API
                ↓
         1. Read full request
         2. Transform Claude → Gemini
         3. Make HTTP request with correct Content-Length
         4. Stream response back as SSE
```

## Implementation Phases

### Phase 1: Core Server Setup
- Set up axum server with POST /v1/messages endpoint
- Parse configuration from environment
- Add basic error handling

### Phase 2: Request Transformation
- Read and parse Claude format request
- Validate request
- Transform to Gemini format
- Keep existing transform/models/streaming modules

### Phase 3: Gemini Client
- Use reqwest to make POST request to Gemini
- Set proper headers (Content-Type, Content-Length, x-goog-api-key)
- Handle authentication

### Phase 4: Response Streaming
- Parse Gemini's streaming JSON response
- Convert to Claude SSE format using existing SSEEventGenerator
- Stream back to client with proper SSE headers

### Phase 5: Error Handling & Logging
- Handle Gemini API errors
- Add tracing for requests/responses
- Return proper HTTP status codes

## Key Advantages Over Pingora

1. ✅ Full control over request body - can read, transform, and send with correct Content-Length
2. ✅ Simple, straightforward code - no complex filter chains
3. ✅ Works with all Gemini endpoints including streamGenerateContent
4. ✅ Easier to debug and maintain

## File Structure

```
src/
├── main.rs           # Axum server setup
├── handlers.rs       # HTTP request handlers (NEW)
├── client.rs         # Gemini HTTP client (NEW)
├── config.rs         # Keep existing
├── error.rs          # Keep existing
├── models/           # Keep existing
├── streaming/        # Keep existing
└── transform/        # Keep existing
```

## Dependencies

- `axum` - Web framework
- `reqwest` - HTTP client with streaming support
- `tokio` - Async runtime (already have)
- `tower-http` - Middleware for CORS, logging
- Keep: `serde`, `serde_json`, `bytes`, `tracing`
- Remove: `pingora`

## Implementation Notes

- Axum handlers are simple async functions - no trait implementations
- Reqwest supports streaming responses natively
- Can set Content-Length easily before making request
- Full control over every header and body byte
