use bytes::Bytes;

use crate::error::ProxyError;

#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
    #[allow(dead_code)]
    pub id: Option<String>,
    #[allow(dead_code)]
    pub retry: Option<u64>,
}

#[derive(Default)]
struct PartialEvent {
    event: Option<String>,
    data: Vec<String>,
    id: Option<String>,
    retry: Option<u64>,
}

#[derive(Default)]
pub struct SseParser {
    buffer: String,
    current: PartialEvent,
}

impl SseParser {
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<SseEvent>, ProxyError> {
        let text = std::str::from_utf8(bytes).map_err(|err| {
            ProxyError::StreamParse(format!("invalid utf-8 in SSE stream: {err}"))
        })?;
        self.buffer.push_str(text);

        let mut events = Vec::new();
        while let Some(line_end) = self.buffer.find('\n') {
            let mut line = self.buffer.drain(..=line_end).collect::<String>();
            if line.ends_with('\n') {
                line.pop();
            }
            if line.ends_with('\r') {
                line.pop();
            }
            if let Some(event) = self.push_line(&line)? {
                events.push(event);
            }
        }
        Ok(events)
    }

    pub fn finish(&mut self) -> Result<Option<SseEvent>, ProxyError> {
        if !self.buffer.is_empty() {
            let line = std::mem::take(&mut self.buffer);
            if let Some(event) = self.push_line(line.trim_end_matches('\r'))? {
                return Ok(Some(event));
            }
        }
        self.emit()
    }

    fn push_line(&mut self, line: &str) -> Result<Option<SseEvent>, ProxyError> {
        if line.is_empty() {
            return self.emit();
        }
        if line.starts_with(':') {
            return Ok(None);
        }

        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line, ""),
        };

        match field {
            "event" => self.current.event = Some(value.to_string()),
            "data" => self.current.data.push(value.to_string()),
            "id" => self.current.id = Some(value.to_string()),
            "retry" => {
                if let Ok(retry) = value.parse() {
                    self.current.retry = Some(retry);
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn emit(&mut self) -> Result<Option<SseEvent>, ProxyError> {
        if self.current.event.is_none()
            && self.current.data.is_empty()
            && self.current.id.is_none()
            && self.current.retry.is_none()
        {
            return Ok(None);
        }

        let current = std::mem::take(&mut self.current);
        Ok(Some(SseEvent {
            event: current.event,
            data: current.data.join("\n"),
            id: current.id,
            retry: current.retry,
        }))
    }
}

#[derive(Debug, Clone)]
pub struct OutboundSseEvent {
    pub event: Option<String>,
    pub data: String,
}

pub fn encode_sse(event: Option<&str>, data: &str) -> Bytes {
    let mut out = String::new();
    if let Some(event) = event {
        out.push_str("event: ");
        out.push_str(event);
        out.push('\n');
    }
    for line in data.lines() {
        out.push_str("data: ");
        out.push_str(line);
        out.push('\n');
    }
    if data.is_empty() {
        out.push_str("data: \n");
    }
    out.push('\n');
    Bytes::from(out)
}
