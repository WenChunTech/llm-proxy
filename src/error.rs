use salvo::http::StatusCode;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("model not configured: {model}")]
    ModelNotConfigured {
        model: String,
        attempted: Vec<String>,
    },
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
    #[error("configuration error: {0}")]
    Config(String),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl ProxyError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidRequest(_) | Self::ModelNotConfigured { .. } => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::AllProvidersExhausted | Self::Upstream(_) => StatusCode::BAD_GATEWAY,
            Self::Config(_)
            | Self::RequestConversion(_)
            | Self::ResponseConversion(_)
            | Self::StreamParse(_)
            | Self::Json(_)
            | Self::Http(_)
            | Self::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn error_body(&self) -> serde_json::Value {
        let (message, code) = match self {
            Self::ModelNotConfigured { model, .. } => (
                format!("Model '{model}' not found in any provider configuration"),
                "model_not_found",
            ),
            Self::Unauthorized => ("Invalid API key".to_string(), "invalid_api_key"),
            Self::InvalidRequest(message) => (message.clone(), "invalid_request"),
            Self::AllProvidersExhausted => (
                "All providers exhausted".to_string(),
                "all_providers_exhausted",
            ),
            _ => (self.to_string(), "proxy_error"),
        };

        json!({
            "error": {
                "message": message,
                "type": "invalid_request_error",
                "code": code
            }
        })
    }
}
