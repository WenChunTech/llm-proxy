# llm-proxy Rust 实现设计文档

## 1. 目标

本文档设计一个 Rust 版本的 `llm-proxy` 服务层，用于替代当前 Node/Hono/Bun 实现，同时复用当前仓库中已有的 `converter` crate。

必须实现：

- OpenAI Chat 兼容接口
- OpenAI Responses 兼容接口
- Claude Messages 兼容接口
- Gemini GenerateContent 兼容接口
- provider 路由
- provider 自动切换
- SSE 解析与转换
- 重试逻辑
- 配置文件

非首版必须：

- 前端设置页
- 持久化错误日志
- Redis/KV 配置同步
- OAuth token 刷新
- Gemini CLI/Qwen/iFlow/Codex/Grok provider

## 2. 当前 Node 实现摘要

Node 项目主要链路：

```text
server.ts
  -> handleModelRequest
  -> executeModelRequest
  -> provider registry/model priority
  -> provider.convertRequestTo
  -> provider.fetchResponse
  -> provider.convertResponseTo / convertStreamResponseTo
```

关键行为：

- 入口协议由路由决定，不由 provider 决定。
- 请求体中的 `model` 决定候选 provider。
- `model_priority` 决定 provider 尝试顺序。
- 一个 provider 下可以有多份配置。
- Gemini CLI 还会展开多个 project；Rust 首版不迁移 Gemini CLI。
- Codex/Grok 等 provider 可能有多账号；Rust 首版不迁移。
- `fallback_models` 在代码里是数组链，而 README 示例中是对象映射；Rust 首版按代码行为实现数组链。
- SSE 目前只解析 `data:` 行，边界处理不足；Rust 版应实现完整 SSE parser。

## 3. 设计原则

### 3.1 不照搬 Node 动态模型

Node 版 `Provider` 使用 `unknown` config、`Record<string, unknown>` body 和运行时 switch。Rust 版应使用：

- enum 表示协议和 provider 类型
- trait 表示 provider 行为
- typed config 表示不同 provider 配置
- `serde_json::Value` 只作为协议边界的原始载体
- converter crate 的强类型模型作为转换边界

### 3.2 converter crate 只做转换

`converter` 已经包含请求、响应、流式事件转换。服务层不应把 HTTP、重试、配置逻辑塞进 converter。

### 3.3 请求执行链保持可测试

重试、provider 选择、fallback、SSE 解析都应是独立模块，不和 Salvo handler 强绑定。

### 3.4 首版优先保持功能语义一致

首版不做复杂熔断和指标系统。先实现 Node 版已有行为：

- provider 顺序尝试
- 最多 5 次尝试
- 线性退避
- fallback model 链
- 上游 header 透传过滤
- API key 鉴权

## 4. 模块结构

建议目录：

```text
src/
  main.rs
  app.rs
  state.rs
  error.rs

  config/
    mod.rs
    model.rs
    loader.rs
    validate.rs

  api/
    mod.rs
    openai_chat.rs
    openai_responses.rs
    claude.rs
    gemini.rs
    models.rs

  middleware/
    mod.rs
    auth.rs
    headers.rs

  provider/
    mod.rs
    types.rs
    registry.rs
    executor.rs
    client.rs
    openai_chat.rs
    openai_responses.rs
    claude.rs
    gemini.rs

  protocol/
    mod.rs
    convert.rs
    http.rs

  stream/
    mod.rs
    sse.rs
    parser.rs
    encoder.rs
    convert.rs

  retry/
    mod.rs
    policy.rs
```

## 5. 核心类型

### 5.1 ProviderType

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    Chat,
    Responses,
    Claude,
    Gemini,
}
```

语义：

- `ProviderType` 同时表示客户端入口协议和实际上游 provider 协议。
- 这样可以直接贴合 Node/WASM 中的 `ProviderType` 语义，避免 `ApiType`/`ProviderKind` 两套枚举互相转换。
- 当它用于 handler 入参时，表示客户端期望响应类型。
- 当它用于 attempt target 时，表示实际上游 provider 类型。
- 首版不暴露 `GeminiCli`。

### 5.2 Provider 配置

```rust
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    pub api_key: Option<String>,
    #[serde(default)]
    pub model_priority: Vec<ProviderType>,
    #[serde(default)]
    pub fallback_models: Vec<String>,
    #[serde(default)]
    pub openai_chat: Vec<OpenAiChatConfig>,
    #[serde(default)]
    pub openai_responses: Vec<OpenAiResponsesConfig>,
    #[serde(default)]
    pub claude: Vec<ClaudeConfig>,
    #[serde(default)]
    pub gemini: Vec<GeminiConfig>,
    #[serde(default)]
    pub retry: RetryConfig,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct BaseProviderConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OpenAiChatConfig {
    #[serde(flatten)]
    pub base: BaseProviderConfig,
    pub base_url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OpenAiResponsesConfig {
    #[serde(flatten)]
    pub base: BaseProviderConfig,
    pub base_url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ClaudeConfig {
    #[serde(flatten)]
    pub base: BaseProviderConfig,
    pub base_url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct GeminiConfig {
    #[serde(flatten)]
    pub base: BaseProviderConfig,
    pub base_url: String,
    pub api_key: String,
}
```

### 5.3 运行时 Provider 配置 enum

```rust
#[derive(Debug, Clone)]
pub enum ProviderConfig {
    Chat(OpenAiChatConfig),
    Responses(OpenAiResponsesConfig),
    Claude(ClaudeConfig),
    Gemini(GeminiConfig),
}

impl ProviderConfig {
    pub fn provider_type(&self) -> ProviderType;
    pub fn models(&self) -> &[String];
    pub fn enabled(&self) -> bool;
    pub fn base_url(&self) -> &str;
}
```

这样可以在 executor 中统一处理 config，同时在 provider 内部恢复具体类型。

### 5.4 执行请求参数

```rust
pub struct ExecuteRequest {
    pub request_id: String,
    pub target: ProviderType,
    pub model: String,
    pub is_streaming: bool,
    pub body: serde_json::Value,
    pub original_body: serde_json::Value,
    pub forwarded_headers: HeaderMap,
}
```

语义：

- 这不是一个跨层上下文对象，只是 executor 的入参包，避免函数参数过长。
- handler 负责解析 HTTP 请求，然后构造 `ExecuteRequest` 调用 executor。
- executor 之外不传播这个类型。
- `body` 会随 fallback model 改写 `model` 字段。
- `original_body` 永远保留客户端原始请求，用于日志和错误记录。
- `forwarded_headers` 已经过滤，不包含认证头、host、content-length 等敏感或 hop-by-hop header。

## 6. Provider 抽象

### 6.1 trait 设计

```rust
pub trait Provider {
    type Config;

    fn provider_type(&self) -> ProviderType;

    fn prepare_request(
        &self,
        body: serde_json::Value,
        source: ProviderType,
        is_streaming: bool,
    ) -> Result<serde_json::Value, ProxyError>;

    async fn send_request(
        &self,
        client: &reqwest::Client,
        req: serde_json::Value,
        config: &Self::Config,
        forwarded_headers: &HeaderMap,
    ) -> Result<UpstreamResponse, ProxyError>;

    fn convert_response(
        &self,
        response_body: serde_json::Value,
        target: ProviderType,
    ) -> Result<serde_json::Value, ProxyError>;

    fn new_stream_converter(
        &self,
        target: ProviderType,
    ) -> StreamConverterImpl;
}
```

说明：

- 方法名全部使用 Rust 常见 snake_case 命名。
- `provider_type` 是查询方法，比 `kind` 更明确。
- `prepare_request` 表示“把客户端请求准备成该 provider 的请求”，包含协议转换和 stream flag 处理。
- `send_request` 表示真正发 HTTP 请求；避免使用过宽泛的 `fetch`。
- `convert_response` 是同步函数，因为 converter 当前是 CPU/serde 转换。
- `new_stream_converter` 返回一个新的有状态流转换器；每个流请求独占一个实例。

### 6.2 静态分发优先

首版不需要 `Box<dyn Provider>`。Provider 数量固定且很少，使用 enum 做静态分发更合适：

```rust
pub enum ProviderImpl {
    Chat(OpenAiChatProvider),
    Responses(OpenAiResponsesProvider),
    Claude(ClaudeProvider),
    Gemini(GeminiProvider),
}

impl ProviderImpl {
    pub fn provider_type(&self) -> ProviderType {
        match self {
            Self::Chat(p) => p.provider_type(),
            Self::Responses(p) => p.provider_type(),
            Self::Claude(p) => p.provider_type(),
            Self::Gemini(p) => p.provider_type(),
        }
    }

    pub fn prepare_request(
        &self,
        body: serde_json::Value,
        source: ProviderType,
        is_streaming: bool,
    ) -> Result<serde_json::Value, ProxyError> {
        match self {
            Self::Chat(p) => p.prepare_request(body, source, is_streaming),
            Self::Responses(p) => p.prepare_request(body, source, is_streaming),
            Self::Claude(p) => p.prepare_request(body, source, is_streaming),
            Self::Gemini(p) => p.prepare_request(body, source, is_streaming),
        }
    }

    pub async fn send_request(
        &self,
        client: &reqwest::Client,
        req: serde_json::Value,
        config: &ProviderConfig,
        headers: &HeaderMap,
    ) -> Result<UpstreamResponse, ProxyError> {
        match (self, config) {
            (Self::Chat(p), ProviderConfig::Chat(c)) => {
                p.send_request(client, req, c, headers).await
            }
            (Self::Responses(p), ProviderConfig::Responses(c)) => {
                p.send_request(client, req, c, headers).await
            }
            (Self::Claude(p), ProviderConfig::Claude(c)) => {
                p.send_request(client, req, c, headers).await
            }
            (Self::Gemini(p), ProviderConfig::Gemini(c)) => {
                p.send_request(client, req, c, headers).await
            }
            _ => Err(ProxyError::InvalidRequest(
                "provider/config type mismatch".to_string(),
            )),
        }
    }
}
```

配置也通过 enum 静态匹配：

```rust
pub struct AttemptTarget {
    pub provider_type: ProviderType,
    pub provider_index: usize,
    pub config_index: usize,
    pub config: ProviderConfig,
}
```

取舍：

- 优点：无 trait object、无虚调用、类型关系清楚、编译期更容易发现 provider/config 不匹配。
- 代价：新增 provider 时需要在 `ProviderImpl` 和 `ProviderConfig` 的 match 中补分支。
- 当前 provider 数量固定，新增频率低，这个代价可以接受。

只有在后续需要运行时插件化 provider，或 provider 由外部 crate 动态注册时，才引入 `dyn Provider`。

## 7. Provider Registry

### 7.1 目标

根据 model 获取候选 provider，并按 `model_priority` 排序。

```rust
pub struct ProviderRegistry {
    config: Arc<Config>,
    model_index: HashMap<String, Vec<ProviderType>>,
}
```

### 7.2 构建索引

规则：

1. 只扫描 `enabled != false` 的配置。
2. 一个 model 在同一 provider 中只记录一次。
3. 返回 provider 时按 `model_priority` 排序。
4. `model_priority` 未配置时使用默认顺序：

```rust
const DEFAULT_PRIORITY: &[ProviderType] = &[
    ProviderType::Gemini,
    ProviderType::Chat,
    ProviderType::Responses,
    ProviderType::Claude,
];
```

### 7.3 获取 attempt targets

```rust
impl ProviderRegistry {
    pub fn providers_for_model(&self, model: &str) -> Vec<ProviderType>;

    pub fn attempt_targets(&self, model: &str) -> Vec<AttemptTarget>;
}
```

`attempt_targets` 会展开：

```text
provider type
  -> enabled configs
  -> configs containing model
```

首版不展开 multi-account 和 projects。

## 8. 请求执行器

### 8.1 输入输出

```rust
pub struct ExecuteResult {
    pub provider_type: ProviderType,
    pub response: UpstreamResponse,
}

pub enum UpstreamResponse {
    NonStream {
        status: reqwest::StatusCode,
        headers: HeaderMap,
        body: bytes::Bytes,
    },
    Stream {
        status: reqwest::StatusCode,
        headers: HeaderMap,
        body: reqwest::Response,
    },
}
```

### 8.2 执行流程

```text
execute(req):
  models_to_try = [req.model] + fallback_chain(req.model)
  if none configured: ModelNotConfigured

  retry_budget = 0
  last_result = None

  for model in models_to_try:
    targets = registry.attempt_targets(model)

    loop pass:
      pass_started_before_limit = retry_budget < max_retries

      for target in targets:
        if !should_attempt(pass_started_before_limit, retry_budget):
          break

        retry_budget += 1
        provider = providers.get(target.provider_type)
        body = req.body with model overwritten
        provider_req = provider.prepare_request(body, req.target, req.is_streaming)
        upstream = provider.send_request(...)

        if upstream.status is 2xx:
          return success

        last_result = upstream

      if retry_budget >= max_retries:
        break

      sleep(backoff(retry_budget))

  return last_result or AllProvidersExhausted
```

### 8.3 和 Node 一致的点

- 总尝试预算跨 model/fallback 共享。
- 一个 pass 已经开始后，可以尝试完该 pass 内所有 target。
- fallback model 第一轮应至少允许尝试，即使主模型已经消耗完预算。

### 8.4 可改进点

Node 版对所有非 2xx 都继续尝试。Rust 首版可以先保持一致。后续再引入：

- 401/403 不重试同一配置
- 400 默认不重试
- 429 尊重 `Retry-After`
- provider cooldown

这些改进不建议首版混入，避免行为偏离过大。

## 9. Fallback Model 设计

### 9.1 配置形态

首版采用数组链：

```json
{
  "fallback_models": ["gpt-4o", "claude-sonnet-4", "gemini-2.5-pro"]
}
```

含义：

```text
gpt-4o -> claude-sonnet-4 -> gemini-2.5-pro
```

### 9.2 函数

```rust
pub fn fallback_chain(model: &str, fallback_models: &[String]) -> Result<Vec<String>, ProxyError>;
```

规则：

- 如果 model 不在数组中，无 fallback。
- 如果 model 是最后一个，无 fallback。
- 如果配置出现重复，启动时配置校验失败。

## 10. HTTP Handler 设计

### 10.1 Salvo 路由

```text
POST /v1/chat/completions              -> ProviderType::Chat
POST /v1/responses                     -> ProviderType::Responses
POST /v1/messages                      -> ProviderType::Claude
POST /v1beta/models/{modelName}        -> ProviderType::Gemini
GET  /v1/models                        -> models list
GET  /health                           -> health
```

Gemini 路由解析：

```text
modelName = "gemini-2.5-pro:generateContent"
model = "gemini-2.5-pro"
is_streaming = action == "streamGenerateContent"
```

OpenAI/Claude 路由解析：

```text
model = body.model
is_streaming = body.stream == true
```

### 10.2 handler 统一流程

```text
parse request body
parse model + stream flag
build ExecuteRequest
execute provider request

if upstream status is not 2xx:
  proxy upstream response

if actual provider type == target provider type:
  passthrough response or stream
else:
  convert response or stream
```

## 11. Header 策略

### 11.1 请求转发过滤

不转发：

```text
connection
keep-alive
proxy-authenticate
proxy-authorization
te
trailer
transfer-encoding
upgrade
authorization
content-length
cookie
host
proxy-connection
x-api-key
x-goog-api-key
```

### 11.2 响应透传过滤

不透传：

```text
connection
keep-alive
proxy-authenticate
proxy-authorization
te
trailer
transfer-encoding
upgrade
content-encoding
content-length
```

## 12. 认证中间件

配置存在 `api_key` 时启用。

接受：

```text
Authorization: Bearer <api_key>
x-api-key: <api_key>
x-goog-api-key: <api_key>
```

实现要求：

- 常量时间比较。
- OPTIONS 跳过认证。
- `/v1/*`、`/v1beta/*` 必须保护。
- 是否保护 `/api/*` 取决于是否实现前端设置页；首版无 `/api/*` 可不考虑。

## 13. 协议转换设计

### 13.1 非流式请求转换

用 `serde_json::from_value` 转强类型，再 `Into` 转目标类型，最后 `serde_json::to_value`。

示意：

```rust
pub fn convert_request(
    body: serde_json::Value,
    source: ProviderType,
    target: ProviderType,
) -> Result<serde_json::Value, ProxyError> {
    match (source, target) {
        (ProviderType::Chat, ProviderType::Claude) => {
            let req: converter::models::openai::chat::Request = serde_json::from_value(body)?;
            let out: converter::models::claude::Request = req.into();
            Ok(serde_json::to_value(out)?)
        }
        (ProviderType::Chat, ProviderType::Gemini) => {
            let req: converter::models::openai::chat::Request = serde_json::from_value(body)?;
            let out: converter::models::gemini_cli::Request = req.into();
            let out: converter::models::gemini::Request = out.into();
            Ok(serde_json::to_value(out)?)
        }
        _ if source == target => Ok(body),
        _ => ...
    }
}
```

Gemini 注意点：

- 当前 converter 对 `gemini_cli` 支持更完整。
- 对外 provider 是原生 Gemini 时，可使用 `gemini_cli -> gemini` 的转换链。

### 13.2 非流式响应转换

方向与 request 相反：

```text
actual provider response type -> target api response type
```

例如：

```text
Claude response -> OpenAI Chat response
Gemini response -> GeminiCli response -> OpenAI Responses response
OpenAI Responses response -> Claude response
```

## 14. SSE 设计

### 14.1 SSE Event 类型

```rust
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
    pub id: Option<String>,
    pub retry: Option<u64>,
}
```

### 14.2 Parser 状态机

```rust
pub struct SseParser {
    buffer: String,
    current_event: PartialSseEvent,
}

impl SseParser {
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<SseEvent>, ProxyError>;
    pub fn finish(&mut self) -> Result<Option<SseEvent>, ProxyError>;
}
```

支持：

- chunk 任意切分
- `\n` 和 `\r\n`
- 多行 `data:`
- `event:`
- `id:`
- `retry:`
- 注释行 `:`
- 空行触发 event emit

多行 data 合并规则：

```text
data: a
data: b

=> "a\nb"
```

### 14.3 Encoder

```rust
pub fn encode_sse(event: Option<&str>, data: &str) -> bytes::Bytes;
```

OpenAI Chat 兼容输出：

```text
data: {...}\n\n
data: [DONE]\n\n
```

OpenAI Responses/Claude/Gemini 输出可包含 `event:`，但必须兼容原协议。

### 14.4 Stream converter

首版不使用 `Box<dyn StreamConverter>`，而是用 enum 做静态分发：

```rust
pub enum StreamConverterImpl {
    Passthrough,
    Chat(ProtocolStreamConverter),
    Responses(ProtocolStreamConverter),
    Claude(ProtocolStreamConverter),
    Gemini(ProtocolStreamConverter),
}

pub struct ProtocolStreamConverter {
    source: ProviderType,
    target: ProviderType,
    state: converter::convert::StreamState,
}

impl StreamConverterImpl {
    pub fn source(&self) -> ProviderType;
    pub fn target(&self) -> ProviderType;

    pub fn convert_event(
        &mut self,
        event: SseEvent,
    ) -> Result<Vec<OutboundSseEvent>, ProxyError>;

    pub fn finish(&mut self) -> Result<Vec<OutboundSseEvent>, ProxyError>;
}
```

内部持有：

```rust
converter::convert::StreamState
```

### 14.5 流式转换流程

```text
upstream bytes
  -> SseParser
  -> SseEvent
  -> parse data JSON
  -> converter wrapper { chunk, state }
  -> converted chunks + new state
  -> SseEncoder
  -> Salvo response stream
```

如果 source 和 target 相同：

```text
passthrough bytes
```

但仍建议记录是否已经写出下游 chunk，用于流式 retry 边界。

## 15. 流式重试边界

非流式可以重试直到拿到最终响应。

流式必须区分：

```rust
pub enum StreamStartState {
    NotStarted,
    HeadersReceived,
    FirstDownstreamChunkWritten,
}
```

规则：

- 请求失败，未收到 header：可重试。
- 收到非 2xx：可尝试下一个 provider。
- 收到 2xx，但还没向客户端写任何 chunk，解析/转换失败：可重试。
- 已经向客户端写出 chunk：不可切换 provider，只能结束流并记录错误。

首版可简化：

- executor 只负责拿到 2xx upstream stream。
- handler 开始写流后不再重试。

## 16. Provider 实现细节

### 16.1 OpenAI Chat

URL：

```text
{base_url}/chat/completions
```

Headers：

```text
Content-Type: application/json
Authorization: Bearer {api_key}
```

### 16.2 OpenAI Responses

URL：

```text
{base_url}/responses
```

Headers 同 OpenAI Chat。

### 16.3 Claude

URL：

```text
{base_url}/v1/messages
```

Headers：

```text
Content-Type: application/json
x-api-key: {api_key}
anthropic-version: 2023-06-01
```

### 16.4 Gemini

非流式：

```text
{base_url}/v1beta/models/{model}:generateContent
```

流式：

```text
{base_url}/v1beta/models/{model}:streamGenerateContent?alt=sse
```

Headers：

```text
Content-Type: application/json
x-goog-api-key: {api_key}
Authorization: Bearer {api_key}
```

保持 Node 行为：同时设置 `x-goog-api-key` 和 `Authorization`。

## 17. 错误类型

```rust
#[derive(thiserror::Error, Debug)]
pub enum ProxyError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("model not configured: {model}")]
    ModelNotConfigured { model: String, attempted: Vec<String> },

    #[error("all providers exhausted")]
    AllProvidersExhausted,

    #[error("request conversion failed: {0}")]
    RequestConversion(String),

    #[error("response conversion failed: {0}")]
    ResponseConversion(String),

    #[error("stream parse failed: {0}")]
    StreamParse(String),

    #[error("upstream request failed: {0}")]
    Upstream(String),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Http(#[from] reqwest::Error),
}
```

HTTP 映射：

- `ModelNotConfigured` -> 400，OpenAI 风格错误体
- `InvalidRequest` -> 400
- auth failure -> 401
- conversion failure -> 500
- all exhausted with last upstream response -> proxy last response
- all exhausted without response -> 502

## 18. 配置加载

加载顺序：

1. `APP_CONFIG_JSON`
2. `APP_CONFIG`
3. `config.json`
4. empty config

Node 版本地优先读 `config.json`，部署优先 env/Redis。Rust 首版可以更简单，但应文档化顺序。

建议支持启动参数：

```text
--config config.json
```

如提供 `--config`，则优先级最高。

## 19. 依赖

按要求使用命令添加依赖，不手写版本：

```bash
cargo add converter --path ./converter
cargo add salvo --features sse,cors,timeout,logging
cargo add tokio --features macros,rt-multi-thread,signal
cargo add reqwest --features json,stream,rustls-tls
cargo add serde --features derive
cargo add serde_json serde_yaml config thiserror futures-util bytes tracing
cargo add tracing-subscriber --features env-filter,json
cargo add secrecy rand
```

首版不需要 nightly。

## 20. 测试计划

### 20.1 单元测试

- config parse
- config validation
- fallback chain
- model priority sort
- provider target expansion
- header filtering
- auth key extraction
- SSE parser
- SSE encoder

### 20.2 转换测试

复用 `converter/testcases`：

- Chat -> Claude request
- Chat -> Responses request
- Responses -> Chat request
- Claude -> Chat response
- Responses -> Claude response
- Gemini/GeminiCli 转换
- StreamState 连续 chunk 转换

### 20.3 executor 测试

用 mock provider：

- 第一个 provider 成功
- 第一个 provider 500，第二个成功
- 同 provider 多配置顺序尝试
- max retry 生效
- fallback model 生效
- fallback cycle 配置失败

### 20.4 集成测试

使用 Salvo test client 或本地 server：

- `/v1/chat/completions` 非流式
- `/v1/chat/completions` 流式
- `/v1/responses` 非流式
- `/v1/responses` 流式
- `/v1/messages`
- `/v1beta/models/{model}:generateContent`
- `/v1beta/models/{model}:streamGenerateContent`

## 21. 实施里程碑

### M1: 服务骨架

- Salvo app
- config loader
- auth middleware
- header filtering
- health route

### M2: Provider 路由和非流式

- registry
- executor
- four required providers
- non-stream request/response conversion

### M3: SSE

- parser
- encoder
- passthrough stream
- converted stream
- stream conversion tests

### M4: Retry/Fallback 完整化

- max retries
- backoff
- fallback chain
- upstream error proxy

### M5: 兼容补齐

- `/v1/models`
- structured error response
- request id/tracing
- basic logs

## 22. 抽象取舍

### 保留 Node 语义

- route 决定目标协议
- model 决定 provider
- model_priority 排序
- provider config 数组轮询/展开
- fallback model 链
- provider 原生协议相同则 passthrough

### Rust 化调整

- 统一使用 `ProviderType`，不再区分入口协议类型和 provider 类型
- provider config 用 enum，不用 `unknown`
- provider 和 stream converter 优先用 enum 静态分发，不默认使用 trait object
- conversion 放到独立 protocol 模块
- SSE parser 独立状态机
- executor 不依赖 Salvo 上下文
- handler 只负责 HTTP 入参和响应输出

### 暂不实现

- 前端设置页
- Redis/KV
- OAuth refresh
- multi-account auth
- Gemini CLI projects
- provider cooldown/circuit breaker

这些能力可以在当前抽象上增量添加，不需要推翻核心设计。
