# 性能优化记录

本文件记录已完成的性能优化，按热路径影响排序。每项均通过完整 DoD 门禁
(`cargo check` / `cargo test` / `cargo clippy --all-targets -- -D warnings`，主仓与
converter 仓均含；前端 `bun run build` + `bun run lint`)。

## 流式热路径(converter)

### 1. 缓存 `response_id` / `model` / `created`

- 文件：`converter/src/convert/openai_responses/openai_chat/stream_response.rs`
        `converter/src/convert/grok/openai_chat/stream_response.rs`
        `converter/src/convert/mod.rs`
- 问题：原实现为每个 delta chunk 各跑三遍 6 臂 `match`，delta 分支走 `_` 时调用
  `generate_id("chatcmpl")`，触发 `SystemTime::now()` 系统调用 + `format!` 分配，
  每条流每个 chunk 都生成全新 id。
- 改动：在 `Response*` 生命周期事件上把 `id`/`model`/`created` 缓存进
  `ResponsesToChatState` / `GrokToChatState`(新增 `created: i64` 字段)，后续
  delta 复用稳定值；三个 6 臂 match 收敛为单个 match(每臂 `Some(&e.response)`)。
- 附带正确性修复：OpenAI 客户端期望整条流 `id` 稳定，原先每 chunk 都是新 id。

### 2. 工具参数 O(N²) 重解析消除

- 文件：`converter/src/convert/openai_responses/openai_chat/stream_response.rs`
        `converter/src/convert/grok/openai_chat/stream_response.rs`
        `converter/src/convert/common/json.rs` (新增)
- 问题：每个 `ArgumentsDelta` 都对整个累积参数串做 `serde_json::from_str::<Value>(…)`
  探测是否完整；跨 K 个 chunk、总长 N 字节即为 O(N·K) ≈ O(N²) 且伴随 Value 树分配。
- 改动：新增轻量字节级完整性探测器 `crate::convert::common::json::is_complete`，
  用括号/引号/转义深度状态机替代整串 serde 解析(单次扫描、无堆分配，含单测)。
- 范围：仅两个最高频目标(Chat)；gemini_cli 方向用 `HashMap` 语义探测(非纯完整性)，
  为避免改变语义未改动。

### 3. 消除 per-chunk 常量字符串分配

- 文件：`converter/src/models/openai/chat/response.rs`
        多个 `convert/*/openai_chat/{stream_,}response.rs`(22 + 11 处)
- 问题：`Response.object: String` 与 `ChatCompletionDeltaToolCall.r#type: Option<String>`
  为字符串类型，每个 chunk 都 `"chat.completion.chunk".to_string()` / `"function".to_string()`。
- 改动：两字段改为 `Cow<'static, str>`。构造处用 `.into()` 得 `Cow::Borrowed`(零分配)；
  反序列化上游未知值仍得 `Cow::Owned`，无回归。已确认 `.object` 从未被读取(纯输出字段)。

## 代理运行时(src/)

### 4. SSE 解析器：偏移扫描替代逐行 drain

- 文件：`src/stream/sse.rs`
- 问题：`push` 对每行执行 `buffer.drain(..=line_end).collect::<String>()`，每行一次
  memmove(剩余字节左移)+ 一次 String 分配；含 K 行的大 chunk 为 O(K) 次 memmove。
- 改动：改为偏移游标扫描，行内容以 `&str` 切片零拷贝传入；每个 `push` 末尾仅一次
  `drain(..consumed)`。为此将 `push_line` / `emit` 改为接收 `&mut PartialEvent` 的自由函数，
  使 `buffer`(不可变借)与 `current`(可变借)分字段并存。`encode_sse` 增加
  `with_capacity` 预分配。

## 前端(dashboard)

### 5. 日志行批处理 + 稳定 key

- 文件：`frontend/src/features/logs/useLogsSocket.ts`
        `frontend/src/features/logs/ProcessLogsPanel.tsx`
        `frontend/src/features/logs/types.ts`
        `frontend/src/features/logs/LogsView.tsx`
- 问题：每条 WebSocket 日志都 `setProcessLines` 并对最多 5000 元素数组做 `slice()`
  全拷贝(高频日志流下严重 GC)；且渲染用 `key={`${index}-${slice}`}`，环形缓冲淘汰
  旧行时所有 key 平移、迫使整表重渲染。
- 改动：在 ref 中累积待写行，每 80ms 单次 flush 合并为一次 `setState`；
  `processLines` 由 `string[]` 改为 `ProcessLogLine[]`(`{ id, text }`)，渲染用
  `key={line.id}` 稳定 key；`LogsView` 的 `.join('\n')` 同步适配。

## 代理运行时(src/)(续)

### 6. 仪表盘 dump 列表改用 `spawn_blocking`

- 文件：`src/app/dashboard/logs.rs`
- 问题：`api_debug_dumps` / `api_debug_dump_detail` / `api_debug_dump_file` 三个
  async handler 直接调用同步文件系统函数(`list_dumps` / `read_dump_detail` /
  `read_dump_file_bytes`)，`logs/` 目录下可能存在上千个 dump 子目录，列表+搜索会
  阻塞 Salvo 异步工作线程，影响并发请求。
- 改动：将三处同步文件系统调用包进 `tokio::task::spawn_blocking`，移到阻塞线程池
  执行，`ProxyError`/返回类型均为 `Send` 可跨线程返回。需在任务后仍使用的值
  (`query`/`id`/`file`) 先 `clone()` 再 move 进闭包。

### 7. 请求体大小上限

- 文件：`src/app.rs`
- 问题：`JSON_MAX_SIZE = usize::MAX`，代理与仪表盘路由对请求体无上限，存在内存
  耗尽/DoS 风险。
- 改动：设为 `64 * 1024 * 1024`(64 MB)，足够容纳长上下文 LLM 请求，同时阻止
  无界增长。

## 流式热路径(代理侧)

### 8. `values_to_events` 去除完整 Value 树

- 文件：`src/stream/convert.rs`
- 问题：每个出站 chunk 先 `serde_json::to_value`(构建完整 Value 树、克隆每个字段，
  含大块文本/内容)再 `to_string` 重新序列化，两趟且有每字段分配。
- 改动：改为单趟 `serde_json::to_string(&chunk)`(struct→string，无中间树)；`event`
  字段用最小投影 `EventTypePeek { type: Option<String> }` 经 `from_str` 提取——
  serde 仅分词其余字段、只为 `type` 分配字符串(大内容字段不再被克隆)。serde 保证
  正确性，无自研 tokenizer 风险。

### 9. `converted_stream` 改用 `VecDeque`

- 文件：`src/app/proxy.rs`
- 问题：原实现用 `Vec` + `reverse()` + `pop()` 管理 pending 出站 chunk，每批先收集
  到 `Vec` 再 `reverse()` 后逐个 `pop()`(LIFO 模拟 FIFO)。
- 改动：改为 `VecDeque` + `pop_front()`，FIFO 语义直接、消除 `reverse()` 传递。
  两处 pending 队列(初始缓冲与 batch 内部)均改为 `VecDeque`，注释标注 FIFO 语义。

### 10. debug_dump 流式写解耦阻塞 I/O

- 文件：`src/util/debug_dump.rs`
- 问题：`append_response_chunk` 在异步流式路径(`tee_stream` /
  `converted_stream` 的同步 `.map()` 闭包)中做同步 `file.write_all`，开启
  debug_dump 时每 chunk 阻塞异步工作线程。
- 改动：引入 `std::sync::mpsc` channel + `spawn_blocking` 后台写入线程，异步路径
  仅做 `tx.send(bytes)`(非阻塞 send)，文件 I/O 全部移至阻塞线程池。无 Tokio
  运行时时(如单元测试)回退为同步直写。`append_response_chunk` 签名由 `&[u8]`
  改为 `&Bytes` 以零拷贝传递。

## 配置与启动

### 11. `parse_config_value` 消除整 Value 克隆

- 文件：`src/config/mod.rs`
- 问题：为在反序列化后仍能读取原始 `Value` 中的 `server.bind` 遗留端口，先
  `value.clone()` 再 `serde_json::from_value`，克隆整棵配置 Value 树。
- 改动：反序列化前先用 `value.get(…)` 借用提取 `legacy_port`(一个 `Option<u16>`)，
  再 `serde_json::from_value(value)` 消费 owned Value，消除整串克隆。


## 流式热路径(converter)(续)
### 12. Grok 字符串判别字段去字符串化

- 文件：`converter/src/models/grok/common.rs`
        `converter/src/models/openai/responses/common.rs`
        `converter/src/convert/grok/openai_chat/{stream_,}response.rs`
        `converter/src/convert/grok/openai_responses/request.rs`
        `converter/src/convert/openai_chat/grok/{request,stream_response}.rs`
        `converter/src/convert/openai_responses/grok/{request,response}.rs`
        `converter/src/convert/claude/grok/{request,response,stream_response}.rs`
        `converter/src/convert/gemini_cli/grok/{request,response,stream_response}.rs`
        `converter/src/convert/gemini_cli/openai_responses/response.rs`
        `converter/src/convert/claude/openai_responses/response.rs`
        `converter/tests/test_grok_responses_parse.rs`
- 问题：`converter/src/models/grok/common.rs` 多个 `status`/`*_type` 为
  `String`/`Option<String>`，驱使跨约 15 个转换文件、100+ 处 `.to_string()`
  及反向 `as_deref()` 匹配。
- 改动：将 Grok 的判别字段从 `String`/`Option<String>` 收紧为严格枚举类型，
  直接复用 OpenAI Responses 侧已有枚举(`ItemStatus`/`WebSearchCallStatus`/
  `FileSearchStatus`/`CodeInterpreterStatus`/`McpToolCallStatus`/
  `ImageGenerationCallStatus`/`SearchContextSize`/`ToolChoiceOptions`/
  `UserLocationType`/`SummaryType`)；新增单值枚举(`AnnotationType`/
  `InputMessageType`/`WebSearchSourceType`/`ShellActionType`/
  `ShellEnvironmentType`/`ToolChoiceFunctionType`)及 `ReasoningTextType`。
  所有枚举实现 `as_str(&self)` + `From<&str>`，未知值通过 `From<&str>` 的
  fallback 分支兜底(映射到 `Default` 变体)，避免严格反序列化中断请求。
  转换层 `"foo".to_string()` 全量替换为 `"foo".into()`，`as_deref()` 替换为
  `as_ref().map(|s| s.as_str())`。
- 验证：通过 `docs.x.ai` Responses API 文档确认各状态枚举值域；
  `cargo check` + `cargo test` + `cargo clippy --all-targets -- -D warnings`
  (主仓 + converter 仓)均通过。

## 待办(缓办)

- **Grok metadata 类型对齐**(原分析 #9)：Grok
  `metadata: Option<HashMap<String, Value>>` vs OpenAI `HashMap<String, String>`。
  缓办原因：xAI 文档标注 `metadata` 为"不支持，仅为兼容保留"，实际流量中极少出现；
  metadata 转换每请求仅一次(非每 chunk)，现有转换代码已正确处理类型不匹配
  (Grok→Responses 过滤非字符串值；Grok→Chat 字符串化非字符串值)；性能收益微乎其微
  (每请求 0–2 条目)，风险不匹配，缓办。
