# Deployment Guide

## Prerequisites

- Rust 1.70+ with cargo
- Google Gemini API key
- Claude Code CLI installed

## Installation

### 1. Build from Source

```bash
git clone https://github.com/your-org/claude-code-proxy
cd claude-code-proxy
cargo build --release
```

The binary will be at `target/release/claude-code-proxy`.

### 2. Configuration

Create a `.env` file or set environment variables:

```bash
# Required
export GEMINI_API_KEY="your-gemini-api-key-here"

# Optional (defaults shown)
export PROXY_LISTEN_ADDR="127.0.0.1:8080"
export PROXY_WORKERS="4"
export GEMINI_ENDPOINT="generativelanguage.googleapis.com"
export RUST_LOG="info"
```

#### Getting a Gemini API Key

1. Visit [Google AI Studio](https://makersuite.google.com/app/apikey)
2. Create a new API key
3. Copy the key and set it in your environment

### 3. Start the Proxy

```bash
# Using cargo
cargo run --release

# Or using the binary directly
./target/release/claude-code-proxy
```

You should see:
```
Starting Claude-to-Gemini proxy...
  Listen: 127.0.0.1:8080
  Workers: 4
  Gemini endpoint: generativelanguage.googleapis.com
Proxy ready!
```

### 4. Configure Claude Code

In a new terminal:

```bash
# Point Claude Code to the proxy
export ANTHROPIC_API_URL="http://localhost:8080"

# Use any placeholder value for the API key
export ANTHROPIC_API_KEY="sk-placeholder"

# Start Claude Code
claude-code
```

Claude Code will now use Gemini models transparently!

## Verification

### Test the Proxy Manually

```bash
curl -X POST http://localhost:8080/v1/messages \
  -H "Content-Type: application/json" \
  -H "x-api-key: placeholder" \
  -d '{
    "model": "claude-3-5-sonnet-20241022",
    "messages": [{"role": "user", "content": "Say hello"}],
    "max_tokens": 10,
    "stream": true
  }'
```

You should receive SSE events:
```
event: message_start
data: {"type":"message_start",...}

event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"Hello"}}

...
```

## Production Deployment

### Using systemd

Create `/etc/systemd/system/claude-proxy.service`:

```ini
[Unit]
Description=Claude Code Proxy
After=network.target

[Service]
Type=simple
User=proxy
WorkingDirectory=/opt/claude-code-proxy
Environment="GEMINI_API_KEY=your-key-here"
Environment="PROXY_LISTEN_ADDR=0.0.0.0:8080"
Environment="RUST_LOG=info"
ExecStart=/opt/claude-code-proxy/target/release/claude-code-proxy
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

Enable and start:
```bash
sudo systemctl enable claude-proxy
sudo systemctl start claude-proxy
sudo systemctl status claude-proxy
```

### Using Docker

Create `Dockerfile`:

```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/claude-code-proxy /usr/local/bin/
EXPOSE 8080
CMD ["claude-code-proxy"]
```

Build and run:
```bash
docker build -t claude-proxy .
docker run -p 8080:8080 \
  -e GEMINI_API_KEY="your-key" \
  -e PROXY_LISTEN_ADDR="0.0.0.0:8080" \
  claude-proxy
```

### Behind Reverse Proxy (nginx)

```nginx
server {
    listen 443 ssl http2;
    server_name claude-proxy.example.com;

    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # Important for SSE
        proxy_buffering off;
        proxy_cache off;
        proxy_set_header Connection '';
        chunked_transfer_encoding off;
    }
}
```

## Monitoring

### Logs

The proxy logs to stderr. Redirect to a file:

```bash
cargo run --release 2>&1 | tee proxy.log
```

Log format:
```
Request: /v1/messages -> 200 (model: gemini-3-pro-preview, tokens: 10in/25out)
```

### Health Check

```bash
# Simple health check
curl -I http://localhost:8080/health

# Or check if proxy is responding
curl -X POST http://localhost:8080/v1/messages \
  -H "Content-Type: application/json" \
  -d '{"model":"claude-3-5-sonnet","messages":[{"role":"user","content":"test"}],"max_tokens":1}'
```

## Troubleshooting

### Proxy Won't Start

**Check environment variables:**
```bash
echo $GEMINI_API_KEY
```

**Check port availability:**
```bash
lsof -i :8080
```

### Claude Code Can't Connect

**Verify proxy is running:**
```bash
curl http://localhost:8080
```

**Check environment variables:**
```bash
echo $ANTHROPIC_API_URL
# Should be: http://localhost:8080
```

### Streaming Not Working

**Check SSE headers:**
```bash
curl -v http://localhost:8080/v1/messages \
  -H "Content-Type: application/json" \
  -d '{"model":"claude-3-5-sonnet","messages":[{"role":"user","content":"test"}],"stream":true}'
```

Look for: `Content-Type: text/event-stream`

### API Errors

**Check Gemini API key:**
```bash
# Test directly with Gemini
curl "https://generativelanguage.googleapis.com/v1beta/models/gemini-3-pro-preview:generateContent?key=$GEMINI_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"contents":[{"parts":[{"text":"test"}]}]}'
```

**Check quota:**
- Free tier: 15 requests/minute
- See [Gemini API quotas](https://ai.google.dev/pricing)

## Performance Tuning

### Adjust Workers

For high load, increase workers:
```bash
export PROXY_WORKERS=8  # Match CPU cores
```

### Buffer Sizes

The parser uses adaptive buffers:
- Initial: 8KB
- Maximum: 64KB before reallocation
- Resets between requests

### Connection Pooling

Pingora automatically pools connections to Gemini API.

## Security

### API Key Protection

- Never commit API keys to version control
- Use environment variables or secrets management
- Rotate keys periodically

### Network Security

- Use HTTPS for production (reverse proxy)
- Restrict listening address: `127.0.0.1:8080` (localhost only)
- Use firewall rules to limit access

### Rate Limiting

Consider adding rate limiting at the reverse proxy level:

```nginx
limit_req_zone $binary_remote_addr zone=api:10m rate=10r/s;
limit_req zone=api burst=20;
```

## Backup and Recovery

### Configuration Backup

```bash
# Backup environment
env | grep -E "(GEMINI|PROXY)" > config.backup
```

### Logs Backup

```bash
# Rotate logs daily
logrotate -f /etc/logrotate.d/claude-proxy
```

## Updates

### Update to Latest Version

```bash
git pull origin main
cargo build --release
sudo systemctl restart claude-proxy
```

### Database Schema Changes

Not applicable - proxy is stateless.

## Support

For issues:
1. Check logs: `journalctl -u claude-proxy -f`
2. Run tests: `cargo test`
3. Verify config: `cargo run --example simple_transform`
4. See documentation in `specs/` directory

## License

MIT - See LICENSE.md
