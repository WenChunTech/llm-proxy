use converter::models::{claude, gemini, gemini_cli, grok, openai};
use serde_json::Value;

use crate::{error::ProxyError, provider::types::ProviderType};

pub const SUPPORTED_PROTOCOL_CONVERSIONS: &[(ProviderType, ProviderType)] = &[
    (ProviderType::Chat, ProviderType::Responses),
    (ProviderType::Chat, ProviderType::Claude),
    (ProviderType::Chat, ProviderType::Gemini),
    (ProviderType::Chat, ProviderType::Grok),
    (ProviderType::Responses, ProviderType::Chat),
    (ProviderType::Responses, ProviderType::Claude),
    (ProviderType::Responses, ProviderType::Gemini),
    (ProviderType::Responses, ProviderType::Grok),
    (ProviderType::Claude, ProviderType::Chat),
    (ProviderType::Claude, ProviderType::Responses),
    (ProviderType::Claude, ProviderType::Gemini),
    (ProviderType::Claude, ProviderType::Grok),
    (ProviderType::Gemini, ProviderType::Chat),
    (ProviderType::Gemini, ProviderType::Responses),
    (ProviderType::Gemini, ProviderType::Claude),
    (ProviderType::Gemini, ProviderType::Grok),
    (ProviderType::Grok, ProviderType::Chat),
    (ProviderType::Grok, ProviderType::Responses),
    (ProviderType::Grok, ProviderType::Claude),
    (ProviderType::Grok, ProviderType::Gemini),
];

pub fn convert_request(
    body: Value,
    source: ProviderType,
    target: ProviderType,
) -> Result<Value, ProxyError> {
    if source == target {
        return Ok(body);
    }
    ensure_supported(source, target, ConversionKind::Request)?;

    match (source, target) {
        (ProviderType::Chat, ProviderType::Responses) => {
            map::<openai::chat::Request, openai::responses::Request>(body)
        }
        (ProviderType::Chat, ProviderType::Claude) => {
            map::<openai::chat::Request, claude::Request>(body)
        }
        (ProviderType::Chat, ProviderType::Gemini) => {
            let req: openai::chat::Request = serde_json::from_value(body)?;
            let cli: gemini_cli::Request = req.into();
            let gemini: gemini::Request = cli.into();
            Ok(serde_json::to_value(gemini)?)
        }
        (ProviderType::Responses, ProviderType::Chat) => {
            map::<openai::responses::Request, openai::chat::Request>(body)
        }
        (ProviderType::Responses, ProviderType::Claude) => {
            map::<openai::responses::Request, claude::Request>(body)
        }
        (ProviderType::Responses, ProviderType::Gemini) => {
            let req: openai::responses::Request = serde_json::from_value(body)?;
            let cli: gemini_cli::Request = req.into();
            let gemini: gemini::Request = cli.into();
            Ok(serde_json::to_value(gemini)?)
        }
        (ProviderType::Claude, ProviderType::Chat) => {
            map::<claude::Request, openai::chat::Request>(body)
        }
        (ProviderType::Claude, ProviderType::Responses) => {
            map::<claude::Request, openai::responses::Request>(body)
        }
        (ProviderType::Claude, ProviderType::Gemini) => {
            let req: claude::Request = serde_json::from_value(body)?;
            let cli: gemini_cli::Request = req.into();
            let gemini: gemini::Request = cli.into();
            Ok(serde_json::to_value(gemini)?)
        }
        (ProviderType::Gemini, ProviderType::Chat) => {
            let req: gemini::Request = serde_json::from_value(body)?;
            let cli: gemini_cli::Request = req.into();
            let out: openai::chat::Request = cli.into();
            Ok(serde_json::to_value(out)?)
        }
        (ProviderType::Gemini, ProviderType::Responses) => {
            let req: gemini::Request = serde_json::from_value(body)?;
            let cli: gemini_cli::Request = req.into();
            let out: openai::responses::Request = cli.into();
            Ok(serde_json::to_value(out)?)
        }
        (ProviderType::Gemini, ProviderType::Claude) => {
            let req: gemini::Request = serde_json::from_value(body)?;
            let cli: gemini_cli::Request = req.into();
            let out: claude::Request = cli.into();
            Ok(serde_json::to_value(out)?)
        }
        (ProviderType::Chat, ProviderType::Grok) => {
            map::<openai::chat::Request, grok::Request>(body)
        }
        (ProviderType::Responses, ProviderType::Grok) => {
            map::<openai::responses::Request, grok::Request>(body)
        }
        (ProviderType::Claude, ProviderType::Grok) => {
            map::<claude::Request, grok::Request>(body)
        }
        (ProviderType::Gemini, ProviderType::Grok) => {
            let req: gemini::Request = serde_json::from_value(body)?;
            let cli: gemini_cli::Request = req.into();
            let out: grok::Request = cli.into();
            Ok(serde_json::to_value(out)?)
        }
        (ProviderType::Grok, ProviderType::Chat) => {
            map::<grok::Request, openai::chat::Request>(body)
        }
        (ProviderType::Grok, ProviderType::Responses) => {
            map::<grok::Request, openai::responses::Request>(body)
        }
        (ProviderType::Grok, ProviderType::Claude) => {
            map::<grok::Request, claude::Request>(body)
        }
        (ProviderType::Grok, ProviderType::Gemini) => {
            let req: grok::Request = serde_json::from_value(body)?;
            let cli: gemini_cli::Request = req.into();
            let gemini: gemini::Request = cli.into();
            Ok(serde_json::to_value(gemini)?)
        }
        _ => unreachable!("supported request conversion is not implemented"),
    }
}

pub fn convert_response(
    body: Value,
    source: ProviderType,
    target: ProviderType,
) -> Result<Value, ProxyError> {
    if source == target {
        return Ok(body);
    }
    ensure_supported(source, target, ConversionKind::Response)?;

    match (source, target) {
        (ProviderType::Chat, ProviderType::Responses) => {
            map::<openai::chat::Response, openai::responses::Response>(body)
        }
        (ProviderType::Chat, ProviderType::Claude) => {
            map::<openai::chat::Response, claude::Response>(body)
        }
        (ProviderType::Chat, ProviderType::Gemini) => {
            let resp: openai::chat::Response = serde_json::from_value(body)?;
            let cli: gemini_cli::Response = resp.into();
            let gemini: gemini::Response = cli.into();
            Ok(serde_json::to_value(gemini)?)
        }
        (ProviderType::Responses, ProviderType::Chat) => {
            map::<openai::responses::Response, openai::chat::Response>(body)
        }
        (ProviderType::Responses, ProviderType::Claude) => {
            map::<openai::responses::Response, claude::Response>(body)
        }
        (ProviderType::Responses, ProviderType::Gemini) => {
            let resp: openai::responses::Response = serde_json::from_value(body)?;
            let cli: gemini_cli::Response = resp.into();
            let gemini: gemini::Response = cli.into();
            Ok(serde_json::to_value(gemini)?)
        }
        (ProviderType::Claude, ProviderType::Chat) => {
            map::<claude::Response, openai::chat::Response>(body)
        }
        (ProviderType::Claude, ProviderType::Responses) => {
            map::<claude::Response, openai::responses::Response>(body)
        }
        (ProviderType::Claude, ProviderType::Gemini) => {
            let resp: claude::Response = serde_json::from_value(body)?;
            let cli: gemini_cli::Response = resp.into();
            let gemini: gemini::Response = cli.into();
            Ok(serde_json::to_value(gemini)?)
        }
        (ProviderType::Gemini, ProviderType::Chat) => {
            let resp: gemini::Response = serde_json::from_value(body)?;
            let cli: gemini_cli::Response = resp.into();
            let out: openai::chat::Response = cli.into();
            Ok(serde_json::to_value(out)?)
        }
        (ProviderType::Gemini, ProviderType::Responses) => {
            let resp: gemini::Response = serde_json::from_value(body)?;
            let cli: gemini_cli::Response = resp.into();
            let out: openai::responses::Response = cli.into();
            Ok(serde_json::to_value(out)?)
        }
        (ProviderType::Gemini, ProviderType::Claude) => {
            let resp: gemini::Response = serde_json::from_value(body)?;
            let cli: gemini_cli::Response = resp.into();
            let out: claude::Response = cli.into();
            Ok(serde_json::to_value(out)?)
        }
        (ProviderType::Chat, ProviderType::Grok) => {
            map::<openai::chat::Response, grok::Response>(body)
        }
        (ProviderType::Responses, ProviderType::Grok) => {
            map::<openai::responses::Response, grok::Response>(body)
        }
        (ProviderType::Claude, ProviderType::Grok) => {
            map::<claude::Response, grok::Response>(body)
        }
        (ProviderType::Gemini, ProviderType::Grok) => {
            let resp: gemini::Response = serde_json::from_value(body)?;
            let cli: gemini_cli::Response = resp.into();
            let out: grok::Response = cli.into();
            Ok(serde_json::to_value(out)?)
        }
        (ProviderType::Grok, ProviderType::Chat) => {
            map::<grok::Response, openai::chat::Response>(body)
        }
        (ProviderType::Grok, ProviderType::Responses) => {
            map::<grok::Response, openai::responses::Response>(body)
        }
        (ProviderType::Grok, ProviderType::Claude) => {
            map::<grok::Response, claude::Response>(body)
        }
        (ProviderType::Grok, ProviderType::Gemini) => {
            let resp: grok::Response = serde_json::from_value(body)?;
            let cli: gemini_cli::Response = resp.into();
            let gemini: gemini::Response = cli.into();
            Ok(serde_json::to_value(gemini)?)
        }
        _ => unreachable!("supported response conversion is not implemented"),
    }
}

#[derive(Clone, Copy)]
enum ConversionKind {
    Request,
    Response,
}

fn ensure_supported(
    source: ProviderType,
    target: ProviderType,
    kind: ConversionKind,
) -> Result<(), ProxyError> {
    if SUPPORTED_PROTOCOL_CONVERSIONS.contains(&(source, target)) {
        return Ok(());
    }

    let message = format!(
        "unsupported {} conversion: {source:?} -> {target:?}",
        kind.as_str()
    );
    match kind {
        ConversionKind::Request => Err(ProxyError::RequestConversion(message)),
        ConversionKind::Response => Err(ProxyError::ResponseConversion(message)),
    }
}

impl ConversionKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Response => "response",
        }
    }
}

fn map<I, O>(body: Value) -> Result<Value, ProxyError>
where
    I: serde::de::DeserializeOwned,
    O: From<I> + serde::Serialize,
{
    let input: I = serde_json::from_value(body)?;
    Ok(serde_json::to_value(O::from(input))?)
}
