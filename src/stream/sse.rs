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
        let mut consumed = 0usize;
        // Scan for complete lines without draining per line (avoids a memmove
        // and allocation per line; borrows buffer immutably while mutating the
        // disjoint `current` field).
        while let Some(rel) = self.buffer[consumed..].find('\n') {
            let line_end = consumed + rel;
            let line = self.buffer[consumed..line_end].strip_suffix('\r').unwrap_or(&self.buffer[consumed..line_end]);
            if let Some(event) = Self::push_line(&mut self.current, line)? {
                events.push(event);
            }
            consumed = line_end + 1;
        }
        if consumed > 0 {
            self.buffer.drain(..consumed);
        }
        Ok(events)
    }

    pub fn finish(&mut self) -> Result<Option<SseEvent>, ProxyError> {
        if !self.buffer.is_empty() {
            let line = std::mem::take(&mut self.buffer);
            if let Some(event) = Self::push_line(&mut self.current, line.trim_end_matches('\r'))? {
                return Ok(Some(event));
            }
        }
        Self::emit(&mut self.current)
    }

    fn push_line(
        current: &mut PartialEvent,
        line: &str,
    ) -> Result<Option<SseEvent>, ProxyError> {
        if line.is_empty() {
            return Self::emit(current);
        }
        if line.starts_with(':') {
            return Ok(None);
        }

        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line, ""),
        };

        match field {
            "event" => current.event = Some(value.to_string()),
            "data" => current.data.push(value.to_string()),
            "id" => current.id = Some(value.to_string()),
            "retry" => {
                if let Ok(retry) = value.parse() {
                    current.retry = Some(retry);
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn emit(current: &mut PartialEvent) -> Result<Option<SseEvent>, ProxyError> {
        if current.event.is_none()
            && current.data.is_empty()
            && current.id.is_none()
            && current.retry.is_none()
        {
            return Ok(None);
        }

        let taken = std::mem::take(current);
        Ok(Some(SseEvent {
            event: taken.event,
            data: taken.data.join("\n"),
            id: taken.id,
            retry: taken.retry,
        }))
    }
}

#[derive(Debug, Clone)]
pub struct OutboundSseEvent {
    pub event: Option<String>,
    pub data: String,
}

pub fn encode_sse(event: Option<&str>, data: &str) -> Bytes {
    let mut out = String::with_capacity(data.len() + data.len() / 32 + 32);
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
