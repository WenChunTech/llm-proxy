# LLM Proxy

**Multi-protocol LLM reverse proxy with built-in dashboard**

[中文文档](./README.zh-CN.md)

---

## Features

- **Multi-protocol support**: OpenAI Chat, OpenAI Responses, Claude (Anthropic), Gemini, Codex, and Grok
- **Full protocol conversion**: Convert requests, responses, and SSE streams between any supported protocols
- **Smart routing**: Model-based provider selection with configurable priority and failover chains
- **Multi-account rotation**: Rotate API keys and OAuth credentials per provider
- **Retry & fallback**: Automatic retries with backoff, per-model alias fallbacks, and global fallback models
- **Embedded dashboard**: React SPA served directly from the Rust binary — manage providers, routing, and logs
- **Real-time logs**: Live process output and request/response dumps via WebSocket
- **Auth validation**: Validate Codex/Grok auth credentials with streaming progress
- **Config persistence**: JSON file or Upstash Redis for config storage
- **Dark/Light theme**: System-following theme with manual override
- **Bilingual UI**: English/Chinese with system language detection

## Quick Start

### Prerequisites

- Rust toolchain (edition 2024)
- [Bun](https://bun.sh/) (for frontend development only)

### Build & Run

```bash
# Clone
git clone <repo-url> && cd llm-proxy

# Build (includes frontend)
cargo build --release

# Run with default config
cp config.example.json config.json
cargo run --release -- --config config.json
```

The server listens on `0.0.0.0:3000` by default. The dashboard is available at `http://localhost:3000`.

### Frontend Development

```bash
cd frontend
bun install
bun run dev    # Vite dev server
bun run build  # Production build (embedded by cargo build)
bun run lint   # Oxlint
```

Set `LLM_PROXY_SKIP_FRONTEND_BUILD=1` to skip frontend rebuilds during Rust-only iteration.

## Configuration

Create `config.json` (see `config.example.json`):

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

### Key Fields

| Field | Description |
|---|---|
| `port` | HTTP listen port |
| `api_key` | Proxy/dashboard access key (empty = no auth) |
| `log_level` | Tracing log level: `error`/`warn`/`info`/`debug`/`trace` |
| `model_priority` | Provider attempt order per model |
| `fallback_models` | Global fallback models tried after all providers fail |
| `model_aliases` | Per-model failover chain: `{"model-a": ["model-b", "model-c"]}` |
| `retry` | Max retries and backoff step (milliseconds) |
| `providers` | Provider configs grouped by kind |
| `debug_dump` | Enable request/response dump to disk |

### Provider Kinds

| Kind | Protocol | Default Endpoint |
|---|---|---|
| `openai_chat` | Chat Completions | `/v1/chat/completions` |
| `openai_responses` | Responses API | `/v1/responses` |
| `claude` | Anthropic Messages | `/v1/messages` |
| `gemini` | Google Gen AI | `/v1beta/models/{model}` |
| `codex` | Codex Responses | `/v1/responses` |
| `grok` | xAI Responses | `/v1/responses` |

## API Routes

### Proxy Endpoints

| Route | Protocol |
|---|---|
| `POST /v1/chat/completions` | OpenAI Chat |
| `POST /v1/responses` | OpenAI Responses |
| `POST /v1/messages` | Claude |
| `POST /v1beta/models/{model}` | Gemini |
| `POST /grok/v1/responses` | Grok |
| `GET /v1/models` | List models |
| `GET /v1beta/models` | Gemini models |

### Dashboard API (under `/api/`)

| Route | Method | Description |
|---|---|---|
| `/api/config` | GET/PUT | Read/update configuration |
| `/api/models` | GET | List all models |
| `/api/provider-models` | POST | Fetch upstream provider models |
| `/api/provider-test` | POST | Test a provider connection |
| `/api/codex/validate` | POST | Validate Codex auth |
| `/api/grok/validate` | POST | Validate Grok auth |
| `/api/codex/validate/ws` | WS | Streaming Codex auth validation |
| `/api/grok/validate/ws` | WS | Streaming Grok auth validation |
| `/api/logs` | GET | Snapshot process logs |
| `/api/logs/ws` | WS | Live process log stream |
| `/api/debug-dumps` | GET | List request dumps |
| `/api/debug-dumps/{id}` | GET/DELETE | Dump detail / delete |
| `/api/debug-dumps/archive` | GET | Download all dumps as zip |

## Architecture

```
src/
  main.rs              # Process entry, tracing init
  app.rs               # Salvo router, CORS, health
  app/
    dashboard/         # Dashboard API handlers
    proxy.rs           # Proxy request handlers
    frontend.rs        # Embedded SPA asset serving
  config/              # Config load/validate + Upstash Redis sync
  provider/            # Provider registry, executor, OAuth, HTTP
    request_rewrite/   # Per-provider request rewriting
    response_rewrite/  # Per-provider response rewriting
  protocol/            # Protocol conversion (uses converter crate)
  stream/              # SSE parser + stream conversion
  retry/               # Retry / backoff logic
  middleware/          # Auth + header filtering
  state.rs             # Shared runtime state
  error.rs             # ProxyError

converter/             # Request/response/stream conversion crate
frontend/              # React 19 + TypeScript + Vite SPA
```

### Runtime Flow

1. Entry protocol determined by route (not provider)
2. Request `model` selects candidate providers
3. `model_priority` orders provider attempts
4. Provider failure triggers retry / fallback / next credential slot
5. SSE fully parsed and converted between protocols

## Docker

The Dockerfile packages a prebuilt binary. Build on the host first:

```bash
cargo build --release
cp target/release/llm-proxy docker/amd64/  # or arm64
docker build -t llm-proxy .
docker run -p 3000:3000 -v $(pwd)/config.json:/app/config.json llm-proxy
```

## License

Private project.
