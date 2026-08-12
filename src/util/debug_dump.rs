//! Per-request debug dump: unconverted request/response bodies under timestamped directories.
//!
//! Bodies are stored *before* protocol conversion:
//! - `request.json`: original client request body
//! - `response.*`: raw upstream response body (provider wire format)
//!
//! Model / endpoint / provider live in `meta.json`. Directory names are
//! `{YYYYMMDD_HHMMSS_mmm}` (UTC date + time + milliseconds). Concurrent
//! requests that land in the same millisecond get a numeric suffix.
//!
//! Directory layout (when `debug_dump.enabled` is true):
//! ```text
//! {dir}/{YYYYMMDD_HHMMSS_mmm}/
//!   meta.json
//!   request.json
//!   response.json   # non-stream upstream body
//!   response.sse    # stream upstream chunks (as received from provider)
//! ```

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use bytes::Bytes;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;

use crate::config::DebugDumpConfig;
use crate::provider::types::ProviderType;
use crate::util::dump_hub::{DumpEvent, DumpHub};

#[derive(Debug, Clone)]
pub struct DumpContext {
    pub model: String,
    pub endpoint: String,
    pub provider: String,
    pub is_streaming: bool,
    pub status: Option<u16>,
}

impl DumpContext {
    pub fn new(
        model: impl Into<String>,
        endpoint: ProviderType,
        provider: Option<ProviderType>,
        is_streaming: bool,
    ) -> Self {
        Self {
            model: model.into(),
            endpoint: endpoint.as_str().to_string(),
            provider: provider
                .map(|p| p.as_str().to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            is_streaming,
            status: None,
        }
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.status = Some(status);
        self
    }

    pub fn image(model: impl Into<String>, provider: Option<ProviderType>) -> Self {
        Self {
            model: model.into(),
            endpoint: "images".to_string(),
            provider: provider
                .map(|p| p.as_str().to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            is_streaming: false,
            status: None,
        }
    }
}

pub struct DebugDumpSession {
    dir: PathBuf,
    id: String,
    model: String,
    endpoint: String,
    provider: String,
    is_streaming: bool,
    status: Option<u16>,
    hub: Option<DumpHub>,
    /// When a Tokio runtime is available, stream chunks are forwarded to a
    /// background blocking writer via this channel so the async streaming loop
    /// never blocks on per-chunk disk I/O. `None` (no runtime, e.g. unit tests)
    /// falls back to inline synchronous writes.
    async_tx: Option<std::sync::mpsc::Sender<Bytes>>,
    response_file: Mutex<Option<File>>,
    response_path: Mutex<Option<PathBuf>>,
}

impl DebugDumpSession {
    /// Create a dump session when enabled. Returns `None` when disabled or on I/O failure.
    pub fn begin(
        config: &DebugDumpConfig,
        ctx: &DumpContext,
        hub: Option<DumpHub>,
    ) -> Option<Self> {
        if !config.enabled {
            return None;
        }

        let dir = match create_unique_dump_dir(Path::new(&config.dir)) {
            Ok(dir) => dir,
            Err(error) => {
                tracing::warn!(
                    base_dir = %config.dir,
                    error = %error,
                    "failed to create debug dump directory"
                );
                return None;
            }
        };

        let id = dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| dir.display().to_string());

        // Stream chunks are written by a background blocking task so the async
        // streaming loop is not stalled by per-chunk disk I/O. Falls back to
        // inline synchronous writes when no runtime is present (unit tests).
        let async_tx = match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let (tx, rx) = std::sync::mpsc::channel::<Bytes>();
                let writer_path = dir.join("response.sse");
                handle.spawn_blocking(move || {
                    let mut file = match OpenOptions::new().create(true).append(true).open(&writer_path) {
                        Ok(file) => file,
                        Err(error) => {
                            tracing::warn!(
                                path = %writer_path.display(),
                                error = %error,
                                "failed to open debug dump response stream file"
                            );
                            return;
                        }
                    };
                    while let Ok(bytes) = rx.recv() {
                        if let Err(error) = file.write_all(&bytes) {
                            tracing::warn!(
                                path = %writer_path.display(),
                                error = %error,
                                "failed to append debug dump response chunk"
                            );
                            break;
                        }
                    }
                    let _ = file.flush();
                });
                Some(tx)
            }
            Err(_) => None,
        };

        let session = Self {
            dir: dir.clone(),
            id,
            model: ctx.model.clone(),
            endpoint: ctx.endpoint.clone(),
            provider: ctx.provider.clone(),
            is_streaming: ctx.is_streaming,
            status: ctx.status,
            hub,
            async_tx,
            response_file: Mutex::new(None),
            response_path: Mutex::new(None),
        };

        if let Err(error) = session.write_meta(ctx) {
            tracing::warn!(
                dir = %dir.display(),
                error = %error,
                "failed to write debug dump meta"
            );
        }

        session.publish_created();

        tracing::info!(
            dir = %dir.display(),
            model = %ctx.model,
            endpoint = %ctx.endpoint,
            provider = %ctx.provider,
            is_streaming = ctx.is_streaming,
            "debug dump session created"
        );

        Some(session)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn write_request(&self, body: &Value) {
        if let Err(error) = write_json_file(&self.dir.join("request.json"), body) {
            tracing::warn!(
                dir = %self.dir.display(),
                error = %error,
                "failed to write debug dump request"
            );
            return;
        }
        self.publish_updated();
    }

    pub fn write_response_json(&self, body: &Value) {
        let path = self.dir.join("response.json");
        if let Err(error) = write_json_file(&path, body) {
            tracing::warn!(
                path = %path.display(),
                error = %error,
                "failed to write debug dump response json"
            );
            return;
        }
        self.publish_updated();
    }

    pub fn write_response_bytes(&self, body: &[u8]) {
        // Prefer pretty JSON when the body is valid JSON; otherwise write raw bytes.
        if let Ok(value) = serde_json::from_slice::<Value>(body) {
            self.write_response_json(&value);
            return;
        }
        let path = self.dir.join("response.bin");
        if let Err(error) = fs::write(&path, body) {
            tracing::warn!(
                path = %path.display(),
                error = %error,
                "failed to write debug dump response bytes"
            );
            return;
        }
        self.publish_updated();
    }

    pub fn write_error(&self, error: &str) {
        let payload = serde_json::json!({ "error": error });
        if let Err(io_error) = write_json_file(&self.dir.join("error.json"), &payload) {
            tracing::warn!(
                dir = %self.dir.display(),
                error = %io_error,
                "failed to write debug dump error"
            );
            return;
        }
        self.publish_updated();
    }

    pub fn append_response_chunk(&self, chunk: &Bytes) {
        if chunk.is_empty() {
            return;
        }
        if let Some(tx) = &self.async_tx {
            // Decoupled: hand the chunk to the background blocking writer so
            // the async stream loop is not stalled by per-chunk disk I/O.
            // `Bytes::clone` is a cheap Arc bump (no data copy).
            let _ = tx.send(chunk.clone());
        } else {
            self.write_chunk_sync(chunk);
        }

        if let Some(hub) = self.hub.as_ref()
            && hub.receiver_count() > 0
        {
            let text = String::from_utf8_lossy(chunk).into_owned();
            if !text.is_empty() {
                hub.publish(DumpEvent::Chunk {
                    id: self.id.clone(),
                    file: "response.sse".to_string(),
                    text,
                });
            }
        }
    }

    /// Inline synchronous write used when no Tokio runtime is available.
    fn write_chunk_sync(&self, chunk: &[u8]) {
        let path = {
            let mut path_guard = match self.response_path.lock() {
                Ok(guard) => guard,
                Err(error) => {
                    tracing::warn!(error = %error, "debug dump response path lock poisoned");
                    return;
                }
            };
            path_guard
                .get_or_insert_with(|| self.dir.join("response.sse"))
                .clone()
        };

        let mut guard = match self.response_file.lock() {
            Ok(guard) => guard,
            Err(error) => {
                tracing::warn!(error = %error, "debug dump response file lock poisoned");
                return;
            }
        };
        if guard.is_none() {
            match OpenOptions::new().create(true).append(true).open(&path) {
                Ok(file) => *guard = Some(file),
                Err(error) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %error,
                        "failed to open debug dump response stream file"
                    );
                    return;
                }
            }
        }
        if let Some(file) = guard.as_mut()
            && let Err(error) = file.write_all(chunk)
        {
            tracing::warn!(
                path = %path.display(),
                error = %error,
                "failed to append debug dump response chunk"
            );
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    fn list_files(&self) -> Vec<String> {
        list_dump_files(&self.dir)
    }

    fn publish_created(&self) {
        let Some(hub) = self.hub.as_ref() else {
            return;
        };
        hub.publish(DumpEvent::Created {
            id: self.id.clone(),
            model: self.model.clone(),
            endpoint: self.endpoint.clone(),
            provider: self.provider.clone(),
            is_streaming: self.is_streaming,
            status: self.status,
            files: self.list_files(),
        });
    }

    fn publish_updated(&self) {
        let Some(hub) = self.hub.as_ref() else {
            return;
        };
        hub.publish(DumpEvent::Updated {
            id: self.id.clone(),
            model: self.model.clone(),
            endpoint: self.endpoint.clone(),
            provider: self.provider.clone(),
            is_streaming: self.is_streaming,
            status: self.status,
            files: self.list_files(),
        });
    }

    fn write_meta(&self, ctx: &DumpContext) -> std::io::Result<()> {
        #[derive(Serialize)]
        struct Meta<'a> {
            model: &'a str,
            endpoint: &'a str,
            provider: &'a str,
            is_streaming: bool,
            status: Option<u16>,
            dir: String,
        }
        let meta = Meta {
            model: &ctx.model,
            endpoint: &ctx.endpoint,
            provider: &ctx.provider,
            is_streaming: ctx.is_streaming,
            status: ctx.status,
            dir: self.dir.display().to_string(),
        };
        let raw = serde_json::to_vec_pretty(&meta)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        fs::write(self.dir.join("meta.json"), raw)
    }
}

/// Wrap a byte stream so each successful chunk is also appended to the dump session.
pub fn tee_stream<S>(
    stream: S,
    dump: Option<DebugDumpSession>,
) -> impl futures_util::Stream<Item = Result<Bytes, std::io::Error>> + Send
where
    S: futures_util::Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
{
    let dump = dump.map(std::sync::Arc::new);
    stream.map(move |item| {
        if let (Ok(bytes), Some(session)) = (&item, dump.as_ref()) {
            session.append_response_chunk(bytes);
        }
        item
    })
}

fn write_json_file(path: &Path, value: &Value) -> std::io::Result<()> {
    let raw = serde_json::to_vec_pretty(value)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    fs::write(path, raw)
}

const DUMP_FILE_NAMES: &[&str] = &[
    "meta.json",
    "request.json",
    "response.json",
    "response.sse",
    "response.bin",
    "error.json",
];

pub fn is_allowed_dump_file(name: &str) -> bool {
    DUMP_FILE_NAMES.contains(&name)
}

pub fn list_dump_files(dir: &Path) -> Vec<String> {
    DUMP_FILE_NAMES
        .iter()
        .filter_map(|name| {
            let path = dir.join(name);
            path.is_file().then(|| (*name).to_string())
        })
        .collect()
}

pub fn dump_base_dir(config: &DebugDumpConfig) -> PathBuf {
    PathBuf::from(&config.dir)
}

/// Keep directory components filesystem-safe and reasonably short.
pub fn sanitize_component(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len().min(80));
    for ch in raw.chars() {
        if out.len() >= 80 {
            break;
        }
        let mapped = match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => ch,
            '/' | '\\' | ':' | ' ' | '|' | '*' | '?' | '"' | '<' | '>' => '_',
            _ => '_',
        };
        out.push(mapped);
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Create `{root}/{YYYYMMDD_HHMMSS_mmm}[/_{n}]`, exclusive per concurrent request.
fn create_unique_dump_dir(root: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(root)?;

    // First try pure date+time(+ms). On collision (concurrent requests in the
    // same millisecond), append a small disambiguator while keeping the timestamp prefix.
    for attempt in 0u32..10_000 {
        let stamp = format_timestamp_millis(SystemTime::now());
        let name = if attempt == 0 {
            stamp
        } else {
            format!("{stamp}_{attempt}")
        };
        let path = root.join(name);
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "exhausted unique debug dump directory names",
    ))
}

/// UTC timestamp `YYYYMMDD_HHMMSS_mmm` without external time crates.
fn format_timestamp_millis(time: SystemTime) -> String {
    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let total_secs = duration.as_secs();
    let millis = duration.subsec_millis();
    let secs_of_day = total_secs % 86_400;
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;

    let mut days = total_secs / 86_400;
    let mut year = 1970u64;
    loop {
        let diy = if is_leap(year) { 366 } else { 365 };
        if days < diy {
            break;
        }
        days -= diy;
        year += 1;
    }

    let months = if is_leap(year) {
        [31u64, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u64;
    for &len in &months {
        if days < len {
            break;
        }
        days -= len;
        month += 1;
    }
    let day = days + 1;

    format!("{year:04}{month:02}{day:02}_{hour:02}{minute:02}{second:02}_{millis:03}")
}

fn is_leap(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DebugDumpConfig;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn sanitize_replaces_unsafe_chars() {
        assert_eq!(
            sanitize_component("deepseek-ai/deepseek-v4-flash"),
            "deepseek-ai_deepseek-v4-flash"
        );
        assert_eq!(sanitize_component(""), "unknown");
        assert_eq!(sanitize_component("???"), "unknown");
    }

    #[test]
    fn begin_writes_request_and_response_under_timestamp_dir() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("llm-proxy-debug-dump-{nanos}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let config = DebugDumpConfig {
            enabled: true,
            dir: dir.display().to_string(),
        };
        let ctx = DumpContext::new(
            "gpt-4o",
            ProviderType::Chat,
            Some(ProviderType::Claude),
            false,
        )
        .with_status(200);
        let session = DebugDumpSession::begin(&config, &ctx, None).expect("session");
        session.write_request(&serde_json::json!({"model":"gpt-4o","messages":[]}));
        session.write_response_json(&serde_json::json!({"id":"resp"}));

        let name = session.dir().file_name().unwrap().to_string_lossy();
        // Directory is date+time(+ms), not model/endpoint/provider/seq.
        assert!(
            name.chars().all(|c| c.is_ascii_digit() || c == '_'),
            "unexpected dir name: {name}"
        );
        assert!(!name.contains("gpt-4o"), "{name}");
        assert!(!name.contains("openai_chat"), "{name}");
        assert!(session.dir().join("request.json").is_file());
        assert!(session.dir().join("response.json").is_file());
        assert!(session.dir().join("meta.json").is_file());

        let meta: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(session.dir().join("meta.json")).unwrap())
                .unwrap();
        assert_eq!(meta["model"], "gpt-4o");
        assert_eq!(meta["endpoint"], "openai_chat");
        assert_eq!(meta["provider"], "claude");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_begin_creates_distinct_dirs() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("llm-proxy-debug-dump-conc-{nanos}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let config = DebugDumpConfig {
            enabled: true,
            dir: dir.display().to_string(),
        };
        let ctx = DumpContext::new("m", ProviderType::Chat, Some(ProviderType::Chat), false);

        let sessions: Vec<_> = (0..8)
            .map(|_| DebugDumpSession::begin(&config, &ctx, None).expect("session"))
            .collect();
        let mut names: Vec<_> = sessions
            .iter()
            .map(|s| s.dir().file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names.dedup();
        assert_eq!(
            names.len(),
            8,
            "dirs must be unique under concurrency: {names:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn disabled_config_returns_none() {
        let config = DebugDumpConfig {
            enabled: false,
            dir: "logs".into(),
        };
        let ctx = DumpContext::new("m", ProviderType::Chat, None, false);
        assert!(DebugDumpSession::begin(&config, &ctx, None).is_none());
    }
}
