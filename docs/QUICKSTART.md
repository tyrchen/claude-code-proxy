# Quick Start Guide

**Get Claude Code working with Gemini in 3 minutes!**

## Prerequisites

- Rust installed (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- Claude Code CLI installed
- A Gemini API key (get one at https://aistudio.google.com/apikey)

---

## Step-by-Step Setup

### 1. Get Your Gemini API Key (1 minute)

1. Open https://aistudio.google.com/apikey
2. Click "Create API Key"
3. Copy the key (starts with `AIza...`)

### 2. Build and Start the Proxy (1 minute)

```bash
# Clone the repository
git clone <this-repo>
cd claude-code-proxy

# Build (first time only - takes ~2 minutes)
cargo build --release

# Start the proxy
export ANTHROPIC_AUTH_TOKEN="AIza..."  # Your Gemini API key
cargo run --release
```

**Leave this terminal running!**

### 3. Configure and Use Claude Code (30 seconds)

Open a **new terminal**:

```bash
# Quick setup - copy and paste these lines
export ANTHROPIC_BASE_URL=http://localhost:8080
export ANTHROPIC_AUTH_TOKEN="AIza..."  # Same key from step 2
export ANTHROPIC_MODEL=gemini-3-pro-preview

# Start Claude Code
claude-code
```

**Done!** Claude Code is now using Gemini.

---

## One-Command Setup (Advanced)

### Create Config File

```bash
# Create ~/.claude-gemini
cat > ~/.claude-gemini << 'EOF'
export ANTHROPIC_BASE_URL=http://localhost:8080
export ANTHROPIC_AUTH_TOKEN="AIza..."  # Replace with your key
export ANTHROPIC_MODEL=gemini-3-pro-preview
export ANTHROPIC_DEFAULT_OPUS_MODEL=gemini-3-pro-preview
export ANTHROPIC_DEFAULT_SONNET_MODEL=gemini-3-pro-preview
export ANTHROPIC_DEFAULT_HAIKU_MODEL=gemini-3-pro-preview
export CLAUDE_CODE_SUBAGENT_MODEL=gemini-3-pro-preview
EOF
```

### Use Forever

```bash
# Every time you want to use Claude Code with Gemini:
source ~/.claude-gemini
claude-code
```

Or add to your `~/.bashrc` / `~/.zshrc`:

```bash
# Auto-load Gemini configuration
if [ -f ~/.claude-gemini ]; then
    source ~/.claude-gemini
fi
```

---

## Verification

Test that everything works:

```bash
# In terminal 1 - proxy should be running
# You should see: "Proxy ready!"

# In terminal 2 - test the proxy
curl -X POST http://localhost:8080/v1/messages \
  -H "Content-Type: application/json" \
  -H "x-api-key: test" \
  -d '{
    "model": "claude-3-5-sonnet",
    "messages": [{"role": "user", "content": "Say hello"}],
    "max_tokens": 10,
    "stream": false
  }'

# You should get a JSON response from Gemini!
```

---

## Troubleshooting

### "Neither ANTHROPIC_AUTH_TOKEN nor GEMINI_API_KEY is set"

**Fix**: Set your API key before starting the proxy
```bash
export ANTHROPIC_AUTH_TOKEN="your-key-here"
```

### "Connection refused" when using Claude Code

**Fix**: Make sure the proxy is running in another terminal

### Claude Code still uses Claude models

**Fix**: Check environment variables are set:
```bash
echo $ANTHROPIC_BASE_URL  # Should be: http://localhost:8080
echo $ANTHROPIC_AUTH_TOKEN  # Should be your Gemini key
```

### Slow responses

**Fix**: Try a faster model:
```bash
export ANTHROPIC_MODEL=gemini-3-pro-preview
```

---

## What's Happening Behind the Scenes

```
You type in Claude Code
  ↓
Claude Code sends request to: $ANTHROPIC_BASE_URL (proxy)
  ↓
Proxy translates: Claude format → Gemini format
  ↓
Proxy sends to: Google Gemini API
  ↓
Gemini responds with streaming JSON
  ↓
Proxy translates: Gemini format → Claude SSE format
  ↓
Claude Code receives response
  ↓
You see the result!
```

All of this happens in **< 1 millisecond** thanks to Rust + Pingora!

---

## Next Steps

### Make it Permanent

Add to your shell config (`~/.bashrc` or `~/.zshrc`):

```bash
# Gemini proxy configuration
export ANTHROPIC_BASE_URL=http://localhost:8080
export ANTHROPIC_AUTH_TOKEN="your-gemini-key"
export ANTHROPIC_MODEL=gemini-3-pro-preview
```

### Auto-Start Proxy

Create a systemd service or launchd plist to start the proxy automatically on boot.

See [DEPLOYMENT.md](DEPLOYMENT.md) for details.

### Try Different Models

```bash
# Fastest (free)
export ANTHROPIC_MODEL=gemini-3-pro-preview

# Highest quality
export ANTHROPIC_MODEL=gemini-3-pro-preview

# Latest experimental
export ANTHROPIC_MODEL=gemini-3-pro-preview
```

---

## Need Help?

- **Full Documentation**: See [README_EN.md](README_EN.md) or [README_CN.md](README_CN.md)
- **Deployment Guide**: See [DEPLOYMENT.md](DEPLOYMENT.md)
- **Technical Details**: See `specs/` directory

---

**That's it! Enjoy using Claude Code with Gemini!** 🎉
