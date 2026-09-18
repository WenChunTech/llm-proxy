# LLM Proxy

**多协议 LLM 反向代理，内置管理面板**

[English Documentation](./README.md)

---

## 功能特性

- **多协议支持**：OpenAI Chat、OpenAI Responses、Claude (Anthropic)、Gemini、Codex、Grok
- **完整协议转换**：在任意支持的协议间转换请求、响应和 SSE 流
- **智能路由**：基于模型的提供商选择，可配置优先级和故障转移链
- **多账号轮询**：按提供商轮换 API Key 和 OAuth 凭据
- **重试与回退**：自动退避重试、按模型别名降级、全局备用模型
- **内嵌管理面板**：React SPA 直接由 Rust 二进制提供服务，管理提供商、路由和日志
- **实时日志**：通过 WebSocket 查看进程输出和请求/响应转储
- **认证校验**：流式进度展示 Codex/Grok 认证凭据校验
- **配置持久化**：支持 JSON 文件或 Upstash Redis 存储配置
- **深色/浅色主题**：跟随系统主题，支持手动切换
- **双语界面**：中英文切换，自动检测系统语言

## 快速开始

### 前置条件

- Rust 工具链（edition 2024）
- [Bun](https://bun.sh/)（仅前端开发需要）

### 构建与运行

```bash
# 克隆
git clone <repo-url> && cd llm-proxy

# 构建（包含前端）
cargo build --release

# 使用默认配置运行
cp config.example.json config.json
cargo run --release -- --config config.json
```

默认监听 `0.0.0.0:3000`。管理面板地址：`http://localhost:3000`。

### 前端开发

```bash
cd frontend
bun install
bun run dev    # Vite 开发服务器
bun run build  # 生产构建（由 cargo build 内嵌）
bun run lint   # Oxlint 检查
```

设置 `LLM_PROXY_SKIP_FRONTEND_BUILD=1` 可在仅迭代 Rust 代码时跳过前端构建。

## 配置说明

创建 `config.json`（参考 `config.example.json`）：

```json
{
  "port": 3000,
  "api_key": "",
  "log_level": "info",
  "model_priority": ["codex", "grok", "openai_responses", "claude", "openai_chat", "gemini"],
  "fallback_models": [],
  "model_aliases": {},
  "retry": { "max_retries": 5, "backoff_step_ms": 5000 },
  "providers": {
    "openai_chat": [],
    "openai_responses": [],
    "claude": [],
    "gemini": [],
    "codex": [],
    "grok": []
  },
  "debug_dump": { "enabled": true, "dir": "logs" }
}
```

### 关键字段

| 字段 | 说明 |
|---|---|
| `port` | HTTP 监听端口 |
| `api_key` | 代理/面板访问密钥（空 = 无需认证） |
| `log_level` | 日志等级：`error`/`warn`/`info`/`debug`/`trace` |
| `model_priority` | 每个模型的提供商尝试顺序 |
| `fallback_models` | 所有提供商失败后的全局备用模型 |
| `model_aliases` | 按模型降级链：`{"model-a": ["model-b", "model-c"]}` |
| `retry` | 最大重试次数和退避步长（毫秒） |
| `providers` | 按类型分组的提供商配置 |
| `debug_dump` | 启用请求/响应转储到磁盘 |

### 提供商类型

| 类型 | 协议 | 默认端点 |
|---|---|---|
| `openai_chat` | Chat Completions | `/v1/chat/completions` |
| `openai_responses` | Responses API | `/v1/responses` |
| `claude` | Anthropic Messages | `/v1/messages` |
| `gemini` | Google Gen AI | `/v1beta/models/{model}` |
| `codex` | Codex Responses | `/v1/responses` |
| `grok` | xAI Responses | `/v1/responses` |

## API 路由

### 代理端点

| 路由 | 协议 |
|---|---|
| `POST /v1/chat/completions` | OpenAI Chat |
| `POST /v1/responses` | OpenAI Responses |
| `POST /v1/messages` | Claude |
| `POST /v1beta/models/{model}` | Gemini |
| `POST /grok/v1/responses` | Grok |
| `GET /v1/models` | 列出模型 |
| `GET /v1beta/models` | Gemini 模型 |

### 面板 API（`/api/` 下）

| 路由 | 方法 | 说明 |
|---|---|---|
| `/api/config` | GET/PUT | 读取/更新配置 |
| `/api/models` | GET | 列出所有模型 |
| `/api/provider-models` | POST | 获取上游提供商模型 |
| `/api/provider-test` | POST | 测试提供商连接 |
| `/api/codex/validate` | POST | 校验 Codex 认证 |
| `/api/grok/validate` | POST | 校验 Grok 认证 |
| `/api/codex/validate/ws` | WS | 流式 Codex 认证校验 |
| `/api/grok/validate/ws` | WS | 流式 Grok 认证校验 |
| `/api/logs` | GET | 进程日志快照 |
| `/api/logs/ws` | WS | 实时进程日志流 |
| `/api/debug-dumps` | GET | 请求转储列表 |
| `/api/debug-dumps/{id}` | GET/DELETE | 转储详情 / 删除 |
| `/api/debug-dumps/archive` | GET | 下载所有转储为 zip |

## 架构

```
src/
  main.rs              # 进程入口，日志初始化
  app.rs               # Salvo 路由、CORS、健康检查
  app/
    dashboard/         # 面板 API 处理
    proxy.rs           # 代理请求处理
    frontend.rs        # 内嵌 SPA 资源服务
  config/              # 配置加载/校验 + Upstash Redis 同步
  provider/            # 提供商注册、执行器、OAuth、HTTP
    request_rewrite/   # 按提供商请求改写
    response_rewrite/  # 按提供商响应改写
  protocol/            # 协议转换（使用 converter crate）
  stream/              # SSE 解析 + 流转换
  retry/               # 重试 / 退避逻辑
  middleware/          # 认证 + 请求头过滤
  state.rs             # 共享运行态
  error.rs             # ProxyError

converter/             # 请求/响应/流转换 crate
frontend/              # React 19 + TypeScript + Vite SPA
```

### 运行流程

1. 入口协议由路由决定（而非提供商）
2. 请求的 `model` 选择候选提供商
3. `model_priority` 排序提供商尝试顺序
4. 提供商失败触发重试 / 回退 / 下一个凭据槽
5. SSE 完整解析并在协议间转换

## Docker

Dockerfile 打包预构建的二进制。先在宿主机构建：

```bash
cargo build --release
cp target/release/llm-proxy docker/amd64/  # 或 arm64
docker build -t llm-proxy .
docker run -p 3000:3000 -v $(pwd)/config.json:/app/config.json llm-proxy
```

## 许可

私有项目。
