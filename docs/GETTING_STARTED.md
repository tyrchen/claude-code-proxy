# Getting Started with Claude Code + Gemini

**The easiest way to use Claude Code with Google Gemini models.**

---

## What You Need

1. ✅ Gemini API key (free at https://aistudio.google.com/apikey)
2. ✅ Claude Code installed
3. ✅ This proxy (you're already here!)

---

## The Simplest Way (Copy & Paste)

### Terminal 1: Start the Proxy

```bash
cd claude-code-proxy
export ANTHROPIC_AUTH_TOKEN="your-gemini-api-key-here"
./start-proxy.sh
```

### Terminal 2: Use Claude Code

```bash
source ./setup-claude-code.sh  # This will prompt for your API key
claude-code
```

**That's it!** 🎉

---

## Environment Variables Explained

When you use the setup script, it sets these variables:

```bash
# Where Claude Code should send requests (to the proxy)
export ANTHROPIC_BASE_URL=http://localhost:8080

# Your Gemini API key (Claude Code will pass this to the proxy)
export ANTHROPIC_AUTH_TOKEN="your-gemini-api-key"

# Which Gemini model to use (optional, has smart defaults)
export ANTHROPIC_MODEL=gemini-3-pro-preview

# Model overrides for different Claude model classes (optional)
export ANTHROPIC_DEFAULT_OPUS_MODEL=gemini-3-pro-preview          # For complex tasks
export ANTHROPIC_DEFAULT_SONNET_MODEL=gemini-3-pro-preview  # For balanced tasks
export ANTHROPIC_DEFAULT_HAIKU_MODEL=gemini-3-pro-preview   # For quick tasks
export CLAUDE_CODE_SUBAGENT_MODEL=gemini-3-pro-preview      # For subagents
```

**Key Point**: The `ANTHROPIC_AUTH_TOKEN` should contain your **Gemini** API key, not a Claude key!

---

## Model Selection Guide

### I Want: Speed & Free

```bash
export ANTHROPIC_MODEL=gemini-3-pro-preview
```

- ⚡ Blazingly fast
- 💰 Completely free
- 📝 1M token context
- ✅ **Best for**: Most use cases

### I Want: Maximum Quality

```bash
export ANTHROPIC_MODEL=gemini-3-pro-preview
```

- 🎯 Highest quality
- 📚 2M token context (10x Claude!)
- 💵 Paid (but cheaper than Claude)
- ✅ **Best for**: Complex reasoning, large codebases

### I Want: Automatic (Smart Defaults)

```bash
# Don't set ANTHROPIC_MODEL
# The proxy will automatically choose:
# - gemini-3-pro-preview for opus-class requests (complex tasks)
# - gemini-3-pro-preview for sonnet/haiku (general tasks)
```

---

## Complete Example Session

```bash
# ─── Terminal 1: Start Proxy ───
cd claude-code-proxy

export ANTHROPIC_AUTH_TOKEN="AIzaSyABC123..."
cargo run --release

# Output:
# Starting Claude-to-Gemini proxy...
#   Listen: 127.0.0.1:8080
#   Workers: 4
#   Gemini endpoint: generativelanguage.googleapis.com
# Proxy ready!


# ─── Terminal 2: Use Claude Code ───
export ANTHROPIC_BASE_URL=http://localhost:8080
export ANTHROPIC_AUTH_TOKEN="AIzaSyABC123..."
export ANTHROPIC_MODEL=gemini-3-pro-preview

claude-code

# You'll see Claude Code start normally
# Try a command like:
# > help me write a fibonacci function

# The response comes from Gemini! 🎉
```

---

## Checking It's Working

### Method 1: Look at Proxy Logs

In Terminal 1 (proxy), you should see:
```
Request: /v1/messages -> 200 (model: gemini-3-pro-preview, tokens: 10in/50out)
```

### Method 2: Test Manually

```bash
curl -X POST http://localhost:8080/v1/messages \
  -H "Content-Type: application/json" \
  -H "x-api-key: test" \
  -d '{
    "model": "claude-3-5-sonnet",
    "messages": [{"role": "user", "content": "Hello"}],
    "max_tokens": 10
  }' | head -20
```

You should see JSON with `"type":"message_start"` etc.

---

## FAQ

### Do I need a Claude API key?

**No!** You only need a Gemini API key. Set it as `ANTHROPIC_AUTH_TOKEN`.

### Will this work with all Claude Code features?

**Yes!** All text-based features work:
- ✅ Chat
- ✅ Code generation
- ✅ Refactoring
- ✅ Debugging help
- ✅ Streaming responses
- ✅ All slash commands

Not yet supported:
- ❌ Image/vision features (Gemini supports this, proxy doesn't yet)

### How much does it cost?

**Gemini Free Tier**:
- 15 requests per minute
- 1 million tokens per day
- More than enough for daily development!

**Gemini Paid**: Much cheaper than Claude
- Flash: $0.075 per million tokens
- Pro: $1.25-7 per million tokens
- vs Claude: $3-15 per million tokens

### Is it safe?

**Yes!**
- Proxy runs locally (127.0.0.1)
- Your API key never leaves your machine
- Open source - you can audit the code
- All communication to Google is HTTPS encrypted

### What if I want to go back to Claude?

Just unset the environment variables:

```bash
unset ANTHROPIC_BASE_URL
export ANTHROPIC_AUTH_TOKEN="your-claude-key"
claude-code
```

Or close Claude Code and start a new terminal without sourcing the config.

---

## Pro Tips

### Tip 1: Create an Alias

Add to `~/.bashrc` or `~/.zshrc`:

```bash
alias claude-gemini='source ~/.claude-gemini && claude-code'
```

Then just run: `claude-gemini`

### Tip 2: Switch Models Easily

```bash
# Fast mode
alias claude-fast='export ANTHROPIC_MODEL=gemini-3-pro-preview && claude-code'

# Quality mode
alias claude-pro='export ANTHROPIC_MODEL=gemini-3-pro-preview && claude-code'
```

### Tip 3: Auto-Start Proxy on Login

Add to `~/.bashrc`:

```bash
# Start proxy if not running
if ! pgrep -f "claude-code-proxy" > /dev/null; then
    cd ~/claude-code-proxy
    nohup cargo run --release > ~/proxy.log 2>&1 &
    echo "Started Claude Code proxy"
fi
```

---

## Troubleshooting

### "Connection refused"

→ Proxy isn't running. Start it in Terminal 1.

### "API key invalid"

→ Make sure you're using a **Gemini** key, not a Claude key.

### Responses are slow

→ Try the faster model:
```bash
export ANTHROPIC_MODEL=gemini-3-pro-preview
```

### Want to see what's happening

→ Check proxy logs in Terminal 1. Each request shows:
```
Request: /v1/messages -> 200 (model: gemini-3-pro-preview, tokens: 15in/127out)
```

---

## Next Steps

Once it's working:

1. **Read the full guide**: [README_EN.md](README_EN.md) or [README_CN.md](README_CN.md)
2. **Production deployment**: [DEPLOYMENT.md](DEPLOYMENT.md)
3. **Try different models**: See model list in README
4. **Share with your team!**

---

**Enjoy using Claude Code with Gemini!** 🚀

Questions? Check the [README_EN.md](README_EN.md) or [README_CN.md](README_CN.md).
