# AGENTS.md

Guidance for coding agents working in `llm-proxy`.

## Project Overview

`llm-proxy` is a multi-protocol LLM reverse proxy:

- **Backend**: Rust + Tokio + Salvo (`src/`)
- **Protocol conversion**: local `converter` crate
- **Dashboard**: React 19 + TypeScript + Vite (`frontend/`)
- **Config**: `config.json` (see `config.example.json`)
- **Tests**: integration-style unit tests under `tests/`

Core runtime behavior:

1. Entry protocol is determined by the route, not the provider.
2. Request `model` selects candidate providers.
3. `model_priority` orders provider attempts.
4. Provider failure triggers retry / fallback / next credential slot.
5. SSE is fully parsed and converted between protocols.

## Repository Map

```text
src/
  main.rs              # process entry
  app.rs               # Salvo router, CORS, health
  app/                 # dashboard + proxy + embedded frontend
  config/              # config load/validate + Upstash sync
  provider/            # provider registry, executor, OAuth, HTTP
  protocol/            # request protocol adapters
  stream/              # SSE parser + stream conversion
  retry/               # retry / backoff
  middleware/          # auth + header filtering
  state.rs             # shared runtime state/cursors
  error.rs             # ProxyError

converter/             # request/response/stream conversion crate
frontend/              # dashboard SPA (built into binary via build.rs)
tests/                 # crate-level tests
docs/                  # design notes
```

## Installed Skills To Apply

Use these installed skills when relevant (available under `~/.agents/skills/`):

| Skill | When |
|---|---|
| `rust-best-practices` | writing/reviewing Rust code, ownership, errors, clippy, tests |
| `rust-patterns` | idiomatic ownership, traits, Result/Option, concurrency |
| `rust-async-patterns` | Tokio async, channels, cancellation, avoid blocking/lock-across-await |
| `vercel-react-best-practices` | React component work, re-renders, data fetching, bundle size |

Read skill details only when the task needs them; keep project-local conventions below as higher priority.

## Global Working Rules

- Prefer small, surgical diffs that match surrounding style.
- Do not expand scope beyond the requested change.
- Do not commit unless explicitly asked.
- Do not invent provider behavior; match existing protocol/provider semantics.
- Keep protocol conversion in `converter`; keep HTTP/retry/config in `llm-proxy`.
- Avoid `unwrap()` / `expect()` in production paths; tests may use them.
- Prefer typed enums/structs over ad-hoc stringly maps at internal boundaries.
- `serde_json::Value` is acceptable at protocol edges, not as the primary internal model.

## Definition of Done (Mandatory)

Every completed task must be verified before reporting done. Do not claim completion until all of the following pass:

1. **Compile succeeds**
2. **Tests pass**
3. **`cargo clippy` has no warnings**

Required verification commands (Rust-only iteration):

```bash
LLM_PROXY_SKIP_FRONTEND_BUILD=1 cargo check
LLM_PROXY_SKIP_FRONTEND_BUILD=1 cargo test
LLM_PROXY_SKIP_FRONTEND_BUILD=1 cargo clippy --all-targets -- -D warnings
```

If the change touches `frontend/` or embedded assets, also run:

```bash
cd frontend && bun run build && bun run lint
cargo check
cargo test
cargo clippy --all-targets -- -D warnings
```

Rules:

- Treat clippy warnings as failures (`-D warnings`).
- If a check fails, fix it in the same task; do not leave known breakage.
- Prefer focused tests first, then full `cargo test` before final handoff.
- In the final response, briefly report that compile / tests / clippy all passed.

## Rust Backend Conventions

### Style

- Edition is `2024`.
- Follow existing module layout; add code near related provider/protocol logic.
- Prefer `&str` / slices over owned values in APIs when ownership is not needed.
- Prefer iterators and early returns over deep nesting.
- Use `thiserror` for `ProxyError` and domain errors.
- Propagate with `?`; attach context only when it improves diagnosis.
- Async code must stay non-blocking:
  - no `std::thread::sleep`
  - do not hold locks across `.await`
  - prefer channels / short critical sections for shared state

### Architecture Constraints

- Route decides entry protocol (`OpenAI Chat`, `Responses`, `Claude`, `Gemini`).
- Provider selection is driven by model + `model_priority` + enabled configs.
- Credential rotation / OAuth token refresh belongs in `provider::oauth` / credentials helpers.
- Stream conversion goes through `stream` + `converter`; do not reimplement SSE parsing ad hoc.
- Dashboard APIs live under `src/app/dashboard`; proxy handlers under `src/app/proxy.rs`.
- Shared mutable runtime data (provider cursors, auth cursors, config) goes through `AppState`.

### Testing (Rust)

- Prefer adding/extending tests in `tests/*.rs` for behavior that can be unit-tested without network.
- Name tests by behavior, e.g. `rotate_attempt_targets_starts_with_provider_cursor`.
- Keep pure logic (retry, SSE, registry, rewrite, credentials) independently testable.
- Run focused tests first:

```bash
cargo test <test_name>
cargo test --test sse_tests
cargo test
```

- Frontend build is triggered by `build.rs`. Skip when iterating on Rust-only changes:

```bash
LLM_PROXY_SKIP_FRONTEND_BUILD=1 cargo test
LLM_PROXY_SKIP_FRONTEND_BUILD=1 cargo build
```

### Common Commands (Rust)

```bash
LLM_PROXY_SKIP_FRONTEND_BUILD=1 cargo check
LLM_PROXY_SKIP_FRONTEND_BUILD=1 cargo test
LLM_PROXY_SKIP_FRONTEND_BUILD=1 cargo clippy --all-targets -- -D warnings
cargo run -- --config config.json
```

Task completion gate (must all succeed):

```bash
LLM_PROXY_SKIP_FRONTEND_BUILD=1 cargo check && LLM_PROXY_SKIP_FRONTEND_BUILD=1 cargo test && LLM_PROXY_SKIP_FRONTEND_BUILD=1 cargo clippy --all-targets -- -D warnings
```

## Frontend Conventions (`frontend/`)

Stack: React 19, TypeScript, Vite 8, Oxlint, Bun package manager, React Compiler enabled.

### Layout

```text
frontend/src/
  components/   # shared UI (auth, layout, controls, icons)
  features/     # feature views (providers, routing, logs, modals)
  hooks/        # stateful hooks (config, auth validation, theme, toast)
  lib/          # pure helpers (API, import/export, storage, ws)
  config/       # provider defaults/priority helpers
  types/        # domain types
  App.tsx       # shell composition
```

### Style

- Keep domain types in `types/domain.ts`.
- Keep network/IO helpers in `lib/`; keep React state in hooks/components.
- Prefer feature folders over growing `App.tsx`.
- Prefer derived state during render over redundant `useEffect` syncing.
- Avoid defining components inside components.
- Prefer stable callbacks / functional `setState` when updating lists/maps.
- Do not introduce Next.js-only APIs; this is a Vite SPA served by the Rust binary.
- Use existing CSS classes/patterns in `App.css` / `index.css` unless a visual redesign is requested.

### Frontend Commands

```bash
cd frontend
bun install
bun run dev
bun run build
bun run lint
```

Full product build (embeds frontend):

```bash
cargo build
```

## Config & Runtime Notes

- Default config path is typically `config.json`.
- Important fields: `port`, `api_key`, `model_priority`, `fallback_models`, `model_aliases`, `retry`, `providers`, `debug_dump`.
- Provider kinds: `openai_chat`, `openai_responses`, `claude`, `gemini`, `codex`, `grok`.
- Debug dumps write under `logs/` when enabled.
- Docker image expects a prebuilt binary and mounts/uses `/app/config.json` (`WORKDIR` is `/app`).

## Change Playbooks

### Adding or changing a provider

1. Update typed config in `src/config`.
2. Implement/adjust provider module under `src/provider/`.
3. Wire registry/executor paths and auth/credential selection.
4. Ensure request/response/stream conversion uses `converter` correctly.
5. Add tests for selection/retry/rewrite behavior where possible.
6. Update dashboard provider defaults/types if UI must expose it.

### Changing protocol conversion

1. Prefer changes in `converter` when the transformation is protocol-level.
2. Keep proxy-only transport concerns (headers, retries, SSE framing) in `src/`.
3. Add conversion tests on the converter side and stream tests on the proxy side as needed.

### Dashboard UI changes

1. Put view-specific UI under `frontend/src/features/...`.
2. Put reusable controls under `frontend/src/components/...`.
3. Extend `types/domain.ts` before threading new fields through hooks/API mappers.
4. Preserve auth gate behavior (`checking` / `login` / `ready`).
5. Run `bun run build` (or full `cargo build`) before considering UI done.

## Do Not

- Do not mix converter responsibilities with Salvo handlers.
- Do not break multi-account / multi-config rotation semantics.
- Do not weaken API key auth on dashboard or proxy routes.
- Do not add heavy frontend dependencies without clear need.
- Do not rewrite working provider logic for style-only reasons.
- Do not remove `LLM_PROXY_SKIP_FRONTEND_BUILD` support.

## Review Checklist

- [ ] Protocol semantics preserved (status, headers, stream events, model routing)
- [ ] Errors map cleanly through `ProxyError`
- [ ] No lock-across-await or blocking calls in async paths
- [ ] Tests updated for changed pure logic
- [ ] Frontend types/API mappers stay in sync with backend dashboard payloads
- [ ] `cargo check` succeeds (compile clean)
- [ ] `cargo test` passes
- [ ] `cargo clippy --all-targets -- -D warnings` has zero warnings
- [ ] Frontend lint/build pass when `frontend/` was changed
