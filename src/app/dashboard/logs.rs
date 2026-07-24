//! Dashboard APIs for debug-dump request/response bodies and live process logs.

use std::{
    fs,
    path::{Path, PathBuf},
};

use futures_util::StreamExt;
use salvo::http::{HeaderValue, header};
use salvo::prelude::*;
use salvo::websocket::{Message, WebSocketUpgrade};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::broadcast::error::RecvError;

use crate::{
    error::ProxyError,
    util::{
        DumpHub, LogHub,
        debug_dump::{dump_base_dir, is_allowed_dump_file, list_dump_files},
    },
};

use super::{render_error, state_from_depot};

const MAX_LIST: usize = 500;
const MAX_FILE_BYTES: u64 = 12 * 1024 * 1024;

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
}

/// List recent debug dump sessions (newest first).
#[handler]
pub(in crate::app) async fn api_debug_dumps(depot: &mut Depot, res: &mut Response) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let snapshot = state.snapshot().await;
    let base = dump_base_dir(&snapshot.config.debug_dump);
    match list_dumps(&base) {
        Ok(items) => res.render(Json(json!({
            "enabled": snapshot.config.debug_dump.enabled,
            "dir": snapshot.config.debug_dump.dir,
            "items": items,
        }))),
        Err(error) => render_error(res, error),
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
    match read_dump_detail(&base, &id) {
        Ok(detail) => res.render(Json(detail)),
        Err(error) => render_error(res, error),
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
    match read_dump_file_bytes(&base, &id, &file) {
        Ok((_path, bytes)) => {
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
        Err(error) => render_error(res, error),
    }
}

/// WebSocket: dump lifecycle events + process log lines.
///
/// Messages are JSON objects:
/// - `{ "type":"hello", ... }`
/// - `{ "type":"log", "line":"..." }`
/// - dump events from `DumpEvent` (`created` / `updated` / `chunk`)
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

fn list_dumps(base: &Path) -> Result<Vec<DumpSummary>, ProxyError> {
    if !base.exists() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(base).map_err(|error| {
        ProxyError::Config(format!(
            "failed to read debug dump dir {}: {error}",
            base.display()
        ))
    })?;

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

fn read_dump_detail(base: &Path, id: &str) -> Result<Value, ProxyError> {
    let dir = resolve_dump_dir(base, id)?;
    let meta = read_meta(&dir).unwrap_or_default();
    let files = list_dump_files(&dir);
    let mut file_payloads = Vec::new();
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
fn format_file_content(name: &str, bytes: &[u8]) -> String {
    if name.ends_with(".json") || looks_like_json(bytes) {
        if let Ok(value) = serde_json::from_slice::<Value>(bytes)
            && let Ok(pretty) = serde_json::to_string_pretty(&value)
        {
            return pretty;
        }
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
