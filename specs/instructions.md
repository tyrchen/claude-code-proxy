# Instructions

## generate design spec

Based on ./specs/0001-spec.md, think ultra hard, generate a concrete design spec for the project.

## update readme

write both en and cn readme to guide user how to use it - ideally just a CLI read gemini api token envar (ANTHROPIC_AUTH_TOKEN) and user could
properly set these env and start claude code:

```bash
export ANTHROPIC_BASE_URL=http://localhost:8081
export ANTHROPIC_AUTH_TOKEN=${YOUR_GEMINI_API_KEY}
export ANTHROPIC_MODEL=gemini-3-pro-preview
export ANTHROPIC_DEFAULT_OPUS_MODEL=gemini-3-pro-preview
export ANTHROPIC_DEFAULT_SONNET_MODEL=gemini-3-pro-preview
export ANTHROPIC_DEFAULT_HAIKU_MODEL=gemini-3-pro-preview
export CLAUDE_CODE_SUBAGENT_MODEL=gemini-3-pro-preview
```
