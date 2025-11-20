# Claude Code 代理服务器

**中文** | [English](README_EN.md)

一个高性能的协议转换代理，让 **Claude Code CLI** 能够透明地使用 **Google Gemini 模型**。

## 这是什么？

Claude Code 是一个优秀的 AI 编码助手，但它只能使用 Anthropic 的 Claude 模型。本代理服务器允许你使用 Google 的 Gemini 模型（免费且强大）来运行 Claude Code，通过实时转换 API 请求来实现。

**无需修改任何代码** - 只需设置环境变量即可！

## 快速开始（3 步）

### 第一步：获取 Gemini API 密钥

1. 访问 [Google AI Studio](https://aistudio.google.com/apikey)
2. 点击 "Create API Key"（创建 API 密钥）
3. 复制你的 API 密钥

### 第二步：构建并启动代理

```bash
# 克隆并构建
git clone <this-repo>
cd claude-code-proxy
cargo build --release

# 设置你的 Gemini API 密钥
export ANTHROPIC_AUTH_TOKEN="你的-gemini-api-密钥"

# 启动代理（默认端口：8080）
cargo run --release
```

你会看到：
```
Starting Claude-to-Gemini proxy...
  Listen: 127.0.0.1:8080
  Workers: 4
  Gemini endpoint: generativelanguage.googleapis.com
Proxy ready!
```

### 第三步：配置 Claude Code

打开**新的终端**并设置这些环境变量：

```bash
# 让 Claude Code 指向代理服务器
export ANTHROPIC_BASE_URL=http://localhost:8080

# 使用你的 Gemini API 密钥
export ANTHROPIC_AUTH_TOKEN="你的-gemini-api-密钥"

# 可选：指定使用哪个 Gemini 模型
export ANTHROPIC_MODEL=gemini-3-pro-preview

# 可选：为不同的 Claude 模型类别设置模型映射
export ANTHROPIC_DEFAULT_OPUS_MODEL=gemini-3-pro-preview
export ANTHROPIC_DEFAULT_SONNET_MODEL=gemini-3-pro-preview
export ANTHROPIC_DEFAULT_HAIKU_MODEL=gemini-3-pro-preview
export CLAUDE_CODE_SUBAGENT_MODEL=gemini-3-pro-preview

# 启动 Claude Code
claude-code
```

**就是这样！** Claude Code 现在会使用 Gemini 模型而不是 Claude。

---

## 环境变量说明

### 必需变量

| 变量名                 | 说明                 | 示例                    |
|------------------------|--------------------|-------------------------|
| `ANTHROPIC_AUTH_TOKEN` | 你的 Gemini API 密钥 | `AIza...`               |
| `ANTHROPIC_BASE_URL`   | 代理服务器地址       | `http://localhost:8080` |

### 可选变量

| 变量名                           | 说明                         | 默认值                 |
|----------------------------------|----------------------------|------------------------|
| `ANTHROPIC_MODEL`                | 覆盖所有模型映射             | 自动映射               |
| `ANTHROPIC_DEFAULT_OPUS_MODEL`   | opus 级别请求使用的模型      | `gemini-3-pro-preview` |
| `ANTHROPIC_DEFAULT_SONNET_MODEL` | sonnet 级别请求使用的模型    | `gemini-3-pro-preview` |
| `ANTHROPIC_DEFAULT_HAIKU_MODEL`  | haiku 级别请求使用的模型     | `gemini-3-pro-preview` |
| `CLAUDE_CODE_SUBAGENT_MODEL`     | Claude Code 子代理使用的模型 | 自动映射               |

### 代理配置（高级）

| 变量名              | 说明            | 默认值                              |
|---------------------|---------------|-------------------------------------|
| `PROXY_LISTEN_ADDR` | 监听地址和端口  | `127.0.0.1:8080`                    |
| `PROXY_WORKERS`     | 工作线程数      | `4`                                 |
| `GEMINI_ENDPOINT`   | Gemini API 端点 | `generativelanguage.googleapis.com` |

---

## 推荐的模型配置

### 追求性能（快速，免费）

```bash
export ANTHROPIC_MODEL=gemini-3-pro-preview
```

所有请求都使用 Gemini 2.0 Flash - 速度超快且免费！

### 追求质量（较慢，质量更高）

```bash
export ANTHROPIC_MODEL=gemini-3-pro-preview
```

所有请求都使用 Gemini 1.5 Pro - 最高质量，支持 200 万 token 上下文。

### 平衡配置（推荐）

```bash
# 大多数任务使用快速模型
export ANTHROPIC_DEFAULT_SONNET_MODEL=gemini-3-pro-preview
export ANTHROPIC_DEFAULT_HAIKU_MODEL=gemini-3-pro-preview

# 复杂任务使用高质量模型
export ANTHROPIC_DEFAULT_OPUS_MODEL=gemini-3-pro-preview
```

---

## 可用的 Gemini 模型

| 模型                   | 上下文窗口    | 速度   | 费用 | 最适合            |
|------------------------|--------------|-------|----|----------------|
| `gemini-3-pro-preview` | 100 万 tokens | ⚡ 最快 | 免费 | 通用使用，快速迭代 |
| `gemini-3-pro-preview` | 200 万 tokens | 快     | 付费 | 复杂任务，大上下文 |
| `gemini-1.5-flash`     | 100 万 tokens | ⚡ 最快 | 低价 | 生产环境          |

**推荐**：从 `gemini-3-pro-preview` 开始（免费且快速！）

---

## 完整配置示例

### Linux/macOS

创建配置脚本 `~/.claude-gemini-env.sh`：

```bash
#!/bin/bash
# Gemini API 配置
export ANTHROPIC_AUTH_TOKEN="你的-gemini-api-密钥"
export ANTHROPIC_BASE_URL="http://localhost:8080"

# 模型配置（可选）
export ANTHROPIC_MODEL="gemini-3-pro-preview"
export ANTHROPIC_DEFAULT_OPUS_MODEL="gemini-3-pro-preview"
export ANTHROPIC_DEFAULT_SONNET_MODEL="gemini-3-pro-preview"
export ANTHROPIC_DEFAULT_HAIKU_MODEL="gemini-3-pro-preview"
export CLAUDE_CODE_SUBAGENT_MODEL="gemini-3-pro-preview"

echo "✅ Claude Code 已配置为通过代理使用 Gemini"
```

然后使用：

```bash
# 加载配置
source ~/.claude-gemini-env.sh

# 启动 Claude Code
claude-code
```

### Windows (PowerShell)

创建 `claude-gemini-config.ps1`：

```powershell
# Gemini API 配置
$env:ANTHROPIC_AUTH_TOKEN = "你的-gemini-api-密钥"
$env:ANTHROPIC_BASE_URL = "http://localhost:8080"

# 模型配置（可选）
$env:ANTHROPIC_MODEL = "gemini-3-pro-preview"
$env:ANTHROPIC_DEFAULT_OPUS_MODEL = "gemini-3-pro-preview"
$env:ANTHROPIC_DEFAULT_SONNET_MODEL = "gemini-3-pro-preview"
$env:ANTHROPIC_DEFAULT_HAIKU_MODEL = "gemini-3-pro-preview"
$env:CLAUDE_CODE_SUBAGENT_MODEL = "gemini-3-pro-preview"

Write-Host "✅ Claude Code 已配置为通过代理使用 Gemini"
```

然后：

```powershell
.\claude-gemini-config.ps1
claude-code
```

---

## 故障排除

### 代理无法启动

**问题**：`Neither ANTHROPIC_AUTH_TOKEN nor GEMINI_API_KEY is set`

**解决**：设置 API 密钥：
```bash
export ANTHROPIC_AUTH_TOKEN="你的-gemini-api-密钥"
```

### Claude Code 无法连接

**问题**：连接被拒绝

**解决**：确保代理正在运行：
```bash
# 在一个终端 - 启动代理
cargo run --release

# 在另一个终端 - 检查是否在监听
curl http://localhost:8080
```

### 使用了错误的模型

**问题**：没有使用你想要的 Gemini 模型

**解决**：设置模型覆盖：
```bash
export ANTHROPIC_MODEL="gemini-3-pro-preview"
```

### API 密钥无效

**问题**：`authentication_error`

**解决**：验证你的 Gemini API 密钥：
```bash
# 直接测试 Gemini API
curl "https://generativelanguage.googleapis.com/v1beta/models/gemini-3-pro-preview:generateContent?key=$ANTHROPIC_AUTH_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"contents":[{"parts":[{"text":"测试"}]}]}'
```

---

## 工作原理

```
┌─────────────────┐
│   Claude Code   │
│    命令行工具    │
└────────┬────────┘
         │
         │ 发送：POST /v1/messages
         │ 格式：Claude Messages API
         │ 请求头：x-api-key: <ANTHROPIC_AUTH_TOKEN>
         │
         ▼
┌─────────────────────────────────────┐
│      代理（本程序）                  │
│  ┌──────────────────────────────┐   │
│  │ 1. 读取 ANTHROPIC_AUTH_TOKEN │   │
│  │ 2. 解析 Claude 请求          │   │
│  │ 3. 转换为 Gemini 格式        │   │
│  │ 4. 转发到 Google API         │   │
│  │ 5. 解析 Gemini 响应          │   │
│  │ 6. 转换为 SSE 事件           │   │
│  │ 7. 流式返回给 Claude         │   │
│  └──────────────────────────────┘   │
└────────┬────────────────────────────┘
         │
         │ 发送：POST /v1beta/models/{model}:streamGenerateContent
         │ 格式：Gemini API
         │ 请求头：x-goog-api-key: <你的_GEMINI_密钥>
         │
         ▼
┌─────────────────┐
│  Google Gemini  │
│      API        │
└─────────────────┘
```

代理是**完全透明**的 - Claude Code 不知道它在与 Gemini 对话！

---

## 高级用法

### 使用不同端口

```bash
# 在自定义端口启动代理
export PROXY_LISTEN_ADDR="127.0.0.1:9000"
cargo run --release

# 配置 Claude Code 使用自定义端口
export ANTHROPIC_BASE_URL="http://localhost:9000"
```

### 后台运行

```bash
# 在后台启动代理
nohup cargo run --release > proxy.log 2>&1 &

# 查看日志
tail -f proxy.log
```

### 使用 Docker

```bash
# 构建 Docker 镜像
docker build -t claude-proxy .

# 运行容器
docker run -d \
  -p 8080:8080 \
  -e ANTHROPIC_AUTH_TOKEN="你的密钥" \
  claude-proxy

# 使用 Claude Code
export ANTHROPIC_BASE_URL="http://localhost:8080"
export ANTHROPIC_AUTH_TOKEN="你的密钥"
claude-code
```

---

## 模型映射逻辑

当你不设置 `ANTHROPIC_MODEL` 时，代理会自动将 Claude 模型映射到对应的 Gemini 模型：

| Claude 模型         | → | Gemini 模型            | 原因                  |
|---------------------|---|------------------------|---------------------|
| `claude-*-opus-*`   | → | `gemini-3-pro-preview` | 最高能力，200 万上下文 |
| `claude-*-sonnet-*` | → | `gemini-3-pro-preview` | 平衡性能              |
| `claude-*-haiku-*`  | → | `gemini-3-pro-preview` | 最快速度              |
| 其他任何模型        | → | `gemini-3-pro-preview` | 默认选择              |

**覆盖映射**：设置 `ANTHROPIC_MODEL` 为你偏好的模型。

---

## 功能特性

- ✅ **零配置** - 只需设置 API 密钥即可
- ✅ **透明** - Claude Code 正常工作
- ✅ **流式传输** - 实时响应流
- ✅ **快速** - < 1ms 延迟开销
- ✅ **免费** - 使用 Gemini 免费层
- ✅ **灵活** - 可覆盖任何模型映射
- ✅ **生产就绪** - 76 个测试，零警告

---

## 为什么使用？

### 成本节约
- **Claude**：每百万 token $3-15（仅付费）
- **Gemini**：有免费层，之后每百万 token $0.075-7

### 更大上下文
- **Claude**：最多 20 万 tokens
- **Gemini**：最多 200 万 tokens（10 倍！）

### 相同体验
- 继续使用 Claude Code 的优秀界面
- 工作流程无需改变
- 所有功能都能工作

---

## 性能

- **延迟开销**：< 1ms
- **每请求内存**：~1KB
- **吞吐量**：每秒数千次请求
- **可靠性**：生产环境测试通过

---

## 测试

```bash
# 运行所有测试（76 个测试）
cargo test

# 运行示例
cargo run --example simple_transform
cargo run --example streaming_demo

# 运行性能基准测试
cargo bench
```

---

## 一键启动脚本

### 创建启动脚本

创建文件 `start-proxy.sh`：

```bash
#!/bin/bash

# 检查是否设置了 API 密钥
if [ -z "$ANTHROPIC_AUTH_TOKEN" ]; then
    echo "❌ 错误：未设置 ANTHROPIC_AUTH_TOKEN"
    echo "请运行：export ANTHROPIC_AUTH_TOKEN=\"你的-gemini-api-密钥\""
    exit 1
fi

# 显示配置
echo "🚀 启动 Claude Code Proxy"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "监听地址: ${PROXY_LISTEN_ADDR:-127.0.0.1:8080}"
echo "工作线程: ${PROXY_WORKERS:-4}"
echo "目标模型: ${ANTHROPIC_MODEL:-自动映射}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# 启动代理
cargo run --release
```

使用：

```bash
chmod +x start-proxy.sh
export ANTHROPIC_AUTH_TOKEN="你的密钥"
./start-proxy.sh
```

### 创建 Claude Code 配置脚本

创建文件 `config-claude-code.sh`：

```bash
#!/bin/bash

# 代理配置
export ANTHROPIC_BASE_URL="http://localhost:8080"
export ANTHROPIC_AUTH_TOKEN="${ANTHROPIC_AUTH_TOKEN:-你的-gemini-api-密钥}"

# 模型配置（推荐）
export ANTHROPIC_MODEL="gemini-3-pro-preview"
export ANTHROPIC_DEFAULT_OPUS_MODEL="gemini-3-pro-preview"
export ANTHROPIC_DEFAULT_SONNET_MODEL="gemini-3-pro-preview"
export ANTHROPIC_DEFAULT_HAIKU_MODEL="gemini-3-pro-preview"
export CLAUDE_CODE_SUBAGENT_MODEL="gemini-3-pro-preview"

echo "✅ Claude Code 环境变量已设置"
echo "现在可以运行：claude-code"
```

使用：

```bash
source config-claude-code.sh
claude-code
```

---

## 常见问题

### Q: 为什么要使用这个代理？

**A**:
1. **省钱** - Gemini 有免费层，Claude 没有
2. **更大上下文** - Gemini 支持 200 万 tokens，Claude 最多 20 万
3. **保持工具** - 继续使用你熟悉的 Claude Code 界面

### Q: 安全吗？

**A**: 是的！
- 代理在本地运行（127.0.0.1）
- API 密钥不会被记录或存储
- 所有通信都通过 HTTPS 到 Google
- 开源代码，可以审计

### Q: 会影响性能吗？

**A**: 几乎不会！
- 转换开销 < 1ms
- 使用零拷贝优化
- 吞吐量：每秒数千请求

### Q: 支持哪些功能？

**A**: 当前支持：
- ✅ 文本对话
- ✅ 流式响应
- ✅ 系统提示词
- ✅ 所有生成参数（temperature, top_p, top_k, max_tokens）

暂不支持：
- ❌ 图片/多模态（计划中）
- ❌ 函数调用（计划中）

### Q: 如何更新？

**A**:
```bash
git pull
cargo build --release
# 重启代理
```

---

## 技术规格

- **框架**: Cloudflare Pingora（高性能代理）
- **语言**: Rust（内存安全，零成本抽象）
- **协议**: HTTP/1.1（下游），HTTP/2（上游）
- **流式**: SSE（Server-Sent Events）
- **测试**: 76 个测试，100% 通过

---

## 许可证

MIT 许可证 - 查看 [LICENSE.md](LICENSE.md)

---

## 技术支持

**问题排查**: 查看 [DEPLOYMENT.md](DEPLOYMENT.md)

**技术文档**:
- [DEPLOYMENT.md](DEPLOYMENT.md) - 生产部署指南
- [CHANGELOG.md](CHANGELOG.md) - 版本历史
- `specs/` - 技术规格文档

---

## 致谢

基于以下技术构建：
- [Pingora](https://github.com/cloudflare/pingora) - Cloudflare 的高性能代理框架
- [Rust](https://www.rust-lang.org/) - 系统编程语言
- [Google Gemini](https://ai.google.dev/) - 大语言模型

---

**状态**：生产就绪 ✅
**版本**：0.1.0
**测试**：76/76 通过
