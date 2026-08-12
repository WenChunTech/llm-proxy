//! Dashboard APIs for debug-dump request/response bodies and live process logs.

use std::{
    fs,
    path::{Path, PathBuf},
};

use futures_util::StreamExt;
use salvo::http::{HeaderValue, header};
use salvo::prelude::*;
use salvo::websocket::{Message, WebSocketUpgrade};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::broadcast::error::RecvError;

use crate::{
    error::ProxyError,
    util::{
        DumpHub, LogHub,
        debug_dump::{dump_base_dir, is_allowed_dump_file, list_dump_files},
        dump_hub::DumpEvent,
    },
};

use super::{JSON_MAX_SIZE, render_error, state_from_depot};

const MAX_LIST: usize = 500;
const MAX_FILE_BYTES: u64 = 12 * 1024 * 1024;
const MAX_SEARCH_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
struct DumpSummary {
    id: String,
    model: String,
    endpoint: String,
    provider: String,
    is_streaming: bool,
    status: Option<u16>,
    files: Vec<String>,
    mtime_ms: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    matches: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DeleteDumpsRequest {
    #[serde(default)]
    ids: Vec<String>,
}

/// List recent debug dump sessions (newest first).
///
/// Optional query `q` searches model / provider / endpoint / id / status and
/// dump file contents (request/response bodies).
#[handler]
pub(in crate::app) async fn api_debug_dumps(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let query = req
        .query::<String>("q")
        .unwrap_or_default()
        .trim()
        .to_string();
    let snapshot = state.snapshot().await;
    let base = dump_base_dir(&snapshot.config.debug_dump);
    // The dump directory can contain many entries; listing/reading it is
    // blocking filesystem work, so run it off the async runtime.
    let query_for_task = query.clone();
    let result = tokio::task::spawn_blocking(move || list_dumps(&base, &query_for_task))
        .await
        .map_err(|err| ProxyError::Config(format!("dump listing task failed: {err}")));
    match result {
        Ok(Ok(items)) => res.render(Json(json!({
            "enabled": snapshot.config.debug_dump.enabled,
            "dir": snapshot.config.debug_dump.dir,
            "query": query,
            "items": items,
        }))),
        Ok(Err(error)) | Err(error) => render_error(res, error),
    }
}

/// Fetch one dump session: meta + per-file metadata and text content.
#[handler]
pub(in crate::app) async fn api_debug_dump_detail(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let Some(id) = req.param::<String>("id") else {
        render_error(
            res,
            ProxyError::InvalidRequest("missing dump id".to_string()),
        );
        return;
    };
    let snapshot = state.snapshot().await;
    let base = dump_base_dir(&snapshot.config.debug_dump);
    let result = tokio::task::spawn_blocking(move || read_dump_detail(&base, &id))
        .await
        .map_err(|err| ProxyError::Config(format!("dump detail task failed: {err}")));
    match result {
        Ok(Ok(detail)) => res.render(Json(detail)),
        Ok(Err(error)) | Err(error) => render_error(res, error),
    }
}

/// Download a single dump file as an attachment.
#[handler]
pub(in crate::app) async fn api_debug_dump_file(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let Some(id) = req.param::<String>("id") else {
        render_error(
            res,
            ProxyError::InvalidRequest("missing dump id".to_string()),
        );
        return;
    };
    let Some(file) = req.param::<String>("file") else {
        render_error(
            res,
            ProxyError::InvalidRequest("missing file name".to_string()),
        );
        return;
    };
    let snapshot = state.snapshot().await;
    let base = dump_base_dir(&snapshot.config.debug_dump);
    let id_for_task = id.clone();
    let file_for_task = file.clone();
    let result = tokio::task::spawn_blocking(move || {
        read_dump_file_bytes(&base, &id_for_task, &file_for_task)
    })
    .await
    .map_err(|err| ProxyError::Config(format!("dump file read task failed: {err}")));
    match result {
        Ok(Ok((_path, bytes))) => {
            let filename = format!("{id}_{file}");
            res.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            if let Ok(value) = HeaderValue::from_str(&format!(
                "attachment; filename=\"{}\"",
                filename.replace('"', "")
            )) {
                res.headers_mut().insert(header::CONTENT_DISPOSITION, value);
            }
            res.body(bytes);
        }
        Ok(Err(error)) | Err(error) => render_error(res, error),
    }
}

/// Delete one dump session directory.
#[handler]
pub(in crate::app) async fn api_debug_dump_delete(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let Some(id) = req.param::<String>("id") else {
        render_error(
            res,
            ProxyError::InvalidRequest("missing dump id".to_string()),
        );
        return;
    };
    let snapshot = state.snapshot().await;
    let base = dump_base_dir(&snapshot.config.debug_dump);
    match delete_dump(&base, &id) {
        Ok(()) => {
            state
                .dump_hub
                .publish(DumpEvent::Deleted { id: id.clone() });
            res.render(Json(json!({
                "deleted": [id],
                "failed": [],
            })));
        }
        Err(error) => render_error(res, error),
    }
}

/// Batch-delete dump sessions by id.
#[handler]
pub(in crate::app) async fn api_debug_dumps_delete(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let payload = match req
        .parse_json_with_max_size::<DeleteDumpsRequest>(JSON_MAX_SIZE)
        .await
    {
        Ok(payload) => payload,
        Err(error) => {
            render_error(
                res,
                ProxyError::InvalidRequest(format!("invalid JSON body: {error}")),
            );
            return;
        }
    };
    if payload.ids.is_empty() {
        render_error(
            res,
            ProxyError::InvalidRequest("ids must not be empty".to_string()),
        );
        return;
    }
    let snapshot = state.snapshot().await;
    let base = dump_base_dir(&snapshot.config.debug_dump);
    let result = delete_dumps(&base, &payload.ids);
    for id in &result.deleted {
        state
            .dump_hub
            .publish(DumpEvent::Deleted { id: id.clone() });
    }
    res.render(Json(json!({
        "deleted": result.deleted,
        "failed": result.failed,
    })));
}

/// WebSocket: dump lifecycle events + process log lines.
///
/// Messages are JSON objects:
/// - `{ "type":"hello", ... }`
/// - `{ "type":"log", "line":"..." }`
/// - dump events from `DumpEvent` (`created` / `updated` / `deleted` / `chunk`)
#[handler]
pub(in crate::app) async fn api_logs_ws(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<(), StatusError> {
    let state = state_from_depot(depot).map_err(|_| StatusError::internal_server_error())?;
    let log_hub = state.log_hub.clone();
    let dump_hub = state.dump_hub.clone();
    let snapshot = state.snapshot().await;
    let enabled = snapshot.config.debug_dump.enabled;
    let dir = snapshot.config.debug_dump.dir.clone();

    WebSocketUpgrade::new()
        .upgrade(req, res, move |ws| {
            handle_logs_socket(ws, log_hub, dump_hub, enabled, dir)
        })
        .await
}

async fn handle_logs_socket(
    mut ws: salvo::websocket::WebSocket,
    log_hub: LogHub,
    dump_hub: DumpHub,
    enabled: bool,
    dir: String,
) {
    let mut log_rx = log_hub.subscribe();
    let mut dump_rx = dump_hub.subscribe();

    let hello = json!({
        "type": "hello",
        "debug_dump": { "enabled": enabled, "dir": dir },
        "buffered_logs": log_hub.snapshot().len(),
    });
    if send_json(&mut ws, &hello).await.is_err() {
        return;
    }

    // Replay recent process log lines as structured messages (ANSI stripped).
    for line in log_hub.snapshot() {
        let payload = json!({ "type": "log", "line": strip_ansi(&line) });
        if send_json(&mut ws, &payload).await.is_err() {
            return;
        }
    }

    loop {
        tokio::select! {
            client = ws.next() => {
                match client {
                    Some(Ok(msg)) if msg.is_close() => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => break,
                }
            }
            line = log_rx.recv() => {
                match line {
                    Ok(text) => {
                        let payload = json!({ "type": "log", "line": strip_ansi(&text) });
                        if send_json(&mut ws, &payload).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(_)) => {
                        let payload = json!({
                            "type": "log",
                            "line": "[log stream] lagged; some process log lines were skipped",
                        });
                        if send_json(&mut ws, &payload).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Closed) => break,
                }
            }
            event = dump_rx.recv() => {
                match event {
                    Ok(event) => {
                        if send_json(&mut ws, &event).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(_)) => {
                        let payload = json!({
                            "type": "log",
                            "line": "[dump stream] lagged; some dump events were skipped",
                        });
                        if send_json(&mut ws, &payload).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        }
    }
}

async fn send_json(ws: &mut salvo::websocket::WebSocket, value: &impl Serialize) -> Result<(), ()> {
    let text = serde_json::to_string(value).map_err(|_| ())?;
    ws.send(Message::text(text)).await.map_err(|_| ())
}

fn list_dumps(base: &Path, query: &str) -> Result<Vec<DumpSummary>, ProxyError> {
    if !base.exists() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(base).map_err(|error| {
        ProxyError::Config(format!(
            "failed to read debug dump dir {}: {error}",
            base.display()
        ))
    })?;

    let query = query.trim();
    let mut items = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(id) = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        if !is_safe_id(&id) {
            continue;
        }
        // Prefer directories that look like dumps (have meta.json or request.json).
        let files = list_dump_files(&path);
        if files.is_empty() {
            continue;
        }
        let meta = read_meta(&path).unwrap_or_default();
        let matches = if query.is_empty() {
            Vec::new()
        } else {
            match dump_query_matches(&path, &id, &meta, &files, query) {
                Some(hits) => hits,
                None => continue,
            }
        };
        let mtime_ms = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        items.push(DumpSummary {
            id,
            model: meta.model,
            endpoint: meta.endpoint,
            provider: meta.provider,
            is_streaming: meta.is_streaming,
            status: meta.status,
            files,
            mtime_ms,
            matches,
        });
    }

    items.sort_by(|a, b| b.id.cmp(&a.id));
    if items.len() > MAX_LIST {
        items.truncate(MAX_LIST);
    }
    Ok(items)
}

#[derive(Debug, Default, Clone, serde::Deserialize)]
struct MetaFile {
    #[serde(default)]
    model: String,
    #[serde(default)]
    endpoint: String,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    is_streaming: bool,
    #[serde(default)]
    status: Option<u16>,
}

fn read_meta(dir: &Path) -> Option<MetaFile> {
    let raw = fs::read_to_string(dir.join("meta.json")).ok()?;
    serde_json::from_str(&raw).ok()
}

fn resolve_dump_dir(base: &Path, id: &str) -> Result<PathBuf, ProxyError> {
    if !is_safe_id(id) {
        return Err(ProxyError::InvalidRequest("invalid dump id".to_string()));
    }
    let path = base.join(id);
    if !path.is_dir() {
        return Err(ProxyError::InvalidRequest(format!("dump not found: {id}")));
    }
    Ok(path)
}

fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && !id.contains('/')
        && !id.contains('\\')
        && !id.contains("..")
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.')
}

fn dump_query_matches(
    dir: &Path,
    id: &str,
    meta: &MetaFile,
    files: &[String],
    query: &str,
) -> Option<Vec<String>> {
    let needle = query.to_ascii_lowercase();
    let mut matches = Vec::new();

    if id.to_ascii_lowercase().contains(&needle) {
        matches.push("id".to_string());
    }
    if meta.model.to_ascii_lowercase().contains(&needle) {
        matches.push("model".to_string());
    }
    if meta.provider.to_ascii_lowercase().contains(&needle) {
        matches.push("provider".to_string());
    }
    if meta.endpoint.to_ascii_lowercase().contains(&needle) {
        matches.push("endpoint".to_string());
    }
    if meta
        .status
        .map(|status| status.to_string().contains(&needle))
        .unwrap_or(false)
    {
        matches.push("status".to_string());
    }

    for name in files {
        if file_contains_query(dir, name, &needle) {
            matches.push(name.clone());
        }
    }

    if matches.is_empty() {
        None
    } else {
        Some(matches)
    }
}

fn file_contains_query(dir: &Path, name: &str, needle_lower: &str) -> bool {
    if !is_allowed_dump_file(name) {
        return false;
    }
    let path = dir.join(name);
    let Ok(meta) = fs::metadata(&path) else {
        return false;
    };
    if !meta.is_file() || meta.len() == 0 {
        return false;
    }
    let Ok(bytes) = fs::read(&path) else {
        return false;
    };
    let limit = MAX_SEARCH_FILE_BYTES as usize;
    let slice = if bytes.len() > limit {
        &bytes[..limit]
    } else {
        &bytes[..]
    };
    // Prefer UTF-8 text search; fall back to lossy decode for mixed content.
    if let Ok(text) = std::str::from_utf8(slice) {
        return text.to_ascii_lowercase().contains(needle_lower);
    }
    String::from_utf8_lossy(slice)
        .to_ascii_lowercase()
        .contains(needle_lower)
}

#[derive(Debug, Default)]
struct DeleteResult {
    deleted: Vec<String>,
    failed: Vec<Value>,
}

fn delete_dump(base: &Path, id: &str) -> Result<(), ProxyError> {
    let dir = resolve_dump_dir(base, id)?;
    fs::remove_dir_all(&dir).map_err(|error| {
        ProxyError::Config(format!("failed to delete dump {}: {error}", dir.display()))
    })?;
    Ok(())
}

fn delete_dumps(base: &Path, ids: &[String]) -> DeleteResult {
    let mut result = DeleteResult::default();
    let mut seen = std::collections::HashSet::new();
    for raw in ids {
        let id = raw.trim();
        if id.is_empty() || !seen.insert(id.to_string()) {
            continue;
        }
        match delete_dump(base, id) {
            Ok(()) => result.deleted.push(id.to_string()),
            Err(error) => result.failed.push(json!({
                "id": id,
                "error": error.to_string(),
            })),
        }
    }
    result
}

fn read_dump_detail(base: &Path, id: &str) -> Result<Value, ProxyError> {
    let dir = resolve_dump_dir(base, id)?;
    let meta = read_meta(&dir).unwrap_or_default();
    let files = list_dump_files(&dir);

    let mut file_payloads = Vec::with_capacity(files.len());
    for name in &files {
        let path = dir.join(name);
        let meta_fs = fs::metadata(&path).map_err(|error| {
            ProxyError::Config(format!("failed to stat {}: {error}", path.display()))
        })?;
        let size = meta_fs.len();
        let truncated = size > MAX_FILE_BYTES;
        let content = if size == 0 {
            String::new()
        } else {
            let bytes = fs::read(&path).map_err(|error| {
                ProxyError::Config(format!("failed to read {}: {error}", path.display()))
            })?;
            let slice = if truncated {
                &bytes[..MAX_FILE_BYTES as usize]
            } else {
                &bytes
            };
            format_file_content(name, slice)
        };
        file_payloads.push(json!({
            "name": name,
            "size": size,
            "truncated": truncated,
            "content": content,
            "language": file_language(name),
        }));
    }

    Ok(json!({
        "id": id,
        "model": meta.model,
        "endpoint": meta.endpoint,
        "provider": meta.provider,
        "is_streaming": meta.is_streaming,
        "status": meta.status,
        "files": file_payloads,
    }))
}

fn read_dump_file_bytes(
    base: &Path,
    id: &str,
    file: &str,
) -> Result<(PathBuf, Vec<u8>), ProxyError> {
    if !is_allowed_dump_file(file) {
        return Err(ProxyError::InvalidRequest(format!(
            "unsupported dump file: {file}"
        )));
    }
    let dir = resolve_dump_dir(base, id)?;
    let path = dir.join(file);
    if !path.is_file() {
        return Err(ProxyError::InvalidRequest(format!(
            "file not found: {id}/{file}"
        )));
    }
    let bytes = fs::read(&path).map_err(|error| {
        ProxyError::Config(format!("failed to read {}: {error}", path.display()))
    })?;
    Ok((path, bytes))
}

fn file_language(name: &str) -> &'static str {
    if name.ends_with(".json") {
        "json"
    } else if name.ends_with(".sse") {
        "sse"
    } else {
        "text"
    }
}

/// Pretty-print JSON when possible; otherwise return readable UTF-8 text.
/// Streaming dumps (`.sse`) keep on-disk formatting and are never reformatted.
fn format_file_content(name: &str, bytes: &[u8]) -> String {
    if name.ends_with(".sse") {
        let text = String::from_utf8_lossy(bytes);
        return sanitize_text(&text);
    }
    if (name.ends_with(".json") || looks_like_json(bytes))
        && let Ok(value) = serde_json::from_slice::<Value>(bytes)
        && let Ok(pretty) = serde_json::to_string_pretty(&value)
    {
        return pretty;
    }
    // Strip NULs / replace invalid sequences so the browser never shows binary garbage.
    let text = String::from_utf8_lossy(bytes);
    sanitize_text(&text)
}

fn looks_like_json(bytes: &[u8]) -> bool {
    let trimmed = bytes
        .iter()
        .skip_while(|b| b.is_ascii_whitespace())
        .copied()
        .next();
    matches!(trimmed, Some(b'{' | b'['))
}

fn sanitize_text(input: &str) -> String {
    let stripped = strip_ansi(input);
    stripped
        .chars()
        .map(|ch| match ch {
            '\t' | '\n' | '\r' => ch,
            c if c.is_control() => '�',
            c => c,
        })
        .collect()
}

/// Remove ANSI CSI / OSC sequences that otherwise render as "special characters".
fn strip_ansi(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            i += 1;
            if i < bytes.len() && bytes[i] == b'[' {
                i += 1;
                while i < bytes.len() {
                    let b = bytes[i];
                    i += 1;
                    if (0x40..=0x7e).contains(&b) {
                        break;
                    }
                }
                continue;
            }
            if i < bytes.len() && bytes[i] == b']' {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == 0x07 {
                        i += 1;
                        break;
                    }
                    if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
                continue;
            }
            continue;
        }
        let ch = input[i..].chars().next().unwrap_or('�');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

// Keep old snapshot endpoint as process-log JSON for compatibility.
#[handler]
pub(in crate::app) async fn api_logs_snapshot(depot: &mut Depot, res: &mut Response) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let lines: Vec<String> = state
        .log_hub
        .snapshot()
        .into_iter()
        .map(|line| strip_ansi(&line))
        .collect();
    res.render(Json(json!({
        "lines": lines,
        "count": lines.len(),
        "sequence": state.log_hub.sequence(),
    })));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dump_root() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "llm-proxy-dump-api-{}-{nanos}-{seq}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create root");
        dir
    }

    fn write_dump(root: &Path, id: &str, model: &str, request_body: &str, response_body: &str) {
        let dir = root.join(id);
        fs::create_dir_all(&dir).expect("create dump");
        fs::write(
            dir.join("meta.json"),
            format!(
                r#"{{"model":"{model}","endpoint":"openai_chat","provider":"openai_chat","is_streaming":false,"status":200}}"#
            ),
        )
        .expect("meta");
        fs::write(dir.join("request.json"), request_body).expect("request");
        fs::write(dir.join("response.json"), response_body).expect("response");
    }

    #[test]
    fn list_dumps_filters_by_model_and_body_keyword() {
        let root = temp_dump_root();
        write_dump(
            &root,
            "20260101_000000_001",
            "gpt-test",
            r#"{"messages":[{"role":"user","content":"hello unique-alpha"}]}"#,
            r#"{"choices":[{"message":{"content":"ok"}}]}"#,
        );
        write_dump(
            &root,
            "20260101_000000_002",
            "claude-test",
            r#"{"messages":[{"role":"user","content":"hello unique-beta"}]}"#,
            r#"{"content":[{"text":"done unique-gamma"}]}"#,
        );

        let by_model = list_dumps(&root, "gpt-test").expect("list");
        assert_eq!(by_model.len(), 1);
        assert_eq!(by_model[0].id, "20260101_000000_001");
        assert!(by_model[0].matches.contains(&"model".to_string()));

        let by_request = list_dumps(&root, "unique-beta").expect("list");
        assert_eq!(by_request.len(), 1);
        assert_eq!(by_request[0].id, "20260101_000000_002");
        assert!(by_request[0].matches.contains(&"request.json".to_string()));

        let by_response = list_dumps(&root, "unique-gamma").expect("list");
        assert_eq!(by_response.len(), 1);
        assert!(
            by_response[0]
                .matches
                .contains(&"response.json".to_string())
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn delete_dump_removes_directory_and_batch_skips_missing() {
        let root = temp_dump_root();
        write_dump(
            &root,
            "20260101_000000_010",
            "model-a",
            r#"{"ok":true}"#,
            r#"{"ok":true}"#,
        );
        write_dump(
            &root,
            "20260101_000000_011",
            "model-b",
            r#"{"ok":true}"#,
            r#"{"ok":true}"#,
        );

        delete_dump(&root, "20260101_000000_010").expect("delete one");
        assert!(!root.join("20260101_000000_010").exists());

        let result = delete_dumps(
            &root,
            &[
                "20260101_000000_011".to_string(),
                "missing-id".to_string(),
                "20260101_000000_011".to_string(),
            ],
        );
        assert_eq!(result.deleted, vec!["20260101_000000_011".to_string()]);
        assert_eq!(result.failed.len(), 1);
        assert!(!root.join("20260101_000000_011").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_unsafe_dump_ids() {
        assert!(!is_safe_id("../etc"));
        assert!(!is_safe_id("a/b"));
        assert!(is_safe_id("20260101_000000_001"));
    }
}
