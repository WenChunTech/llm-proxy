use bytes::Bytes;
use futures_util::{StreamExt, TryStreamExt, stream};
use salvo::{
    http::{HeaderValue, StatusCode, header},
    prelude::*,
};
use serde_json::Value;

use crate::{
    error::ProxyError,
    middleware::headers::get_forwardable_request_headers,
    provider::{
        UpstreamResponse,
        executor::{ExecuteImageRequest, ExecuteRequest, bytes_to_json, execute, execute_image},
        types::ProviderType,
    },
    state::AppState,
    stream::{
        convert::StreamContext,
        sse::{SseParser, encode_sse},
    },
    util::debug_dump::{DebugDumpSession, DumpContext, tee_stream},
};

use super::{JSON_MAX_SIZE, apply_headers, dashboard::models_payload, render_error};

#[handler]
pub(super) async fn openai_chat(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    handle_model_request(req, depot, res, ProviderType::Chat).await;
}

#[handler]
pub(super) async fn openai_responses(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    handle_model_request(req, depot, res, ProviderType::Responses).await;
}

#[handler]
pub(super) async fn image_generations(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    handle_image_generation(req, depot, res).await;
}

#[handler]
pub(super) async fn claude_messages(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    handle_model_request(req, depot, res, ProviderType::Claude).await;
}

#[handler]
pub(super) async fn gemini_model(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    handle_model_request(req, depot, res, ProviderType::Gemini).await;
}

#[handler]
pub(super) async fn models(depot: &mut Depot, res: &mut Response) {
    let Some(state) = depot.get::<AppState>("state").ok().cloned() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let snapshot = state.snapshot().await;
    res.render(Json(models_payload(&snapshot)));
}

async fn handle_model_request(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
    target: ProviderType,
) {
    let Some(state) = depot.get::<AppState>("state").ok().cloned() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };

    let request_body = match req.parse_json_with_max_size::<Value>(JSON_MAX_SIZE).await {
        Ok(body) => body,
        Err(error) => {
            render_error(
                res,
                ProxyError::InvalidRequest(format!("invalid JSON body: {error}")),
            );
            return;
        }
    };

    let snapshot = state.snapshot().await;
    let (model, is_streaming, body) = match parse_model_request(req, target, request_body.clone()) {
        Ok(parsed) => parsed,
        Err(error) => {
            render_error(res, error);
            return;
        }
    };

    tracing::info!(
        target = ?target,
        model = %model,
        is_streaming,
        "request entry"
    );

    let stream_context = StreamContext::from_request(target, &request_body);
    let exec_request = ExecuteRequest {
        target,
        model: model.clone(),
        is_streaming,
        body: body.clone(),
        forwarded_headers: get_forwardable_request_headers(req.headers()),
    };

    match execute(&state, &snapshot, exec_request.clone()).await {
        Ok(result) => {
            let dump = DebugDumpSession::begin(
                &snapshot.config.debug_dump,
                &DumpContext::new(&model, target, Some(result.provider_type), is_streaming)
                    .with_status(result.response.status()),
                Some(state.dump_hub.clone()),
            );
            // Dump the original client body before any protocol conversion.
            if let Some(session) = dump.as_ref() {
                session.write_request(&request_body);
            }
            if let Err(error) =
                write_execute_result(res, &state, exec_request, stream_context, result, dump).await
            {
                render_error(res, error);
            }
        }
        Err(error) => {
            if let Some(session) = DebugDumpSession::begin(
                &snapshot.config.debug_dump,
                &DumpContext::new(&model, target, None, is_streaming),
                Some(state.dump_hub.clone()),
            ) {
                session.write_request(&request_body);
                session.write_error(&error.to_string());
            }
            render_error(res, error);
        }
    }
}

async fn handle_image_generation(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(state) = depot.get::<AppState>("state").ok().cloned() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };

    let snapshot = state.snapshot().await;
    let body = match req.parse_json_with_max_size::<Value>(JSON_MAX_SIZE).await {
        Ok(body) => body,
        Err(error) => {
            render_error(
                res,
                ProxyError::InvalidRequest(format!("invalid JSON body: {error}")),
            );
            return;
        }
    };
    let Some(model) = body.get("model").and_then(Value::as_str) else {
        render_error(
            res,
            ProxyError::InvalidRequest("model is required".to_string()),
        );
        return;
    };
    tracing::info!(
        endpoint = "images/generations",
        model = %model,
        "request entry"
    );

    let request = ExecuteImageRequest {
        model: model.to_string(),
        body: body.clone(),
        forwarded_headers: get_forwardable_request_headers(req.headers()),
    };

    match execute_image(&state, &snapshot, request).await {
        Ok(result) => {
            let dump = DebugDumpSession::begin(
                &snapshot.config.debug_dump,
                &DumpContext::image(model, Some(result.provider_type))
                    .with_status(result.response.status()),
                Some(state.dump_hub.clone()),
            );
            if let Some(session) = dump.as_ref() {
                session.write_request(&body);
            }
            apply_headers(res, result.response.headers());
            res.status_code(
                StatusCode::from_u16(result.response.status()).unwrap_or(StatusCode::BAD_GATEWAY),
            );
            write_passthrough_response(res, result.response, dump);
        }
        Err(error) => {
            if let Some(session) = DebugDumpSession::begin(
                &snapshot.config.debug_dump,
                &DumpContext::image(model, None),
                Some(state.dump_hub.clone()),
            ) {
                session.write_request(&body);
                session.write_error(&error.to_string());
            }
            render_error(res, error);
        }
    }
}

fn parse_model_request(
    req: &Request,
    target: ProviderType,
    mut body: Value,
) -> Result<(String, bool, Value), ProxyError> {
    if target == ProviderType::Gemini {
        let model_name = req
            .param::<String>("modelName")
            .ok_or_else(|| ProxyError::InvalidRequest("missing Gemini model path".to_string()))?;
        let (model, action) = model_name.split_once(':').ok_or_else(|| {
            ProxyError::InvalidRequest("Gemini path must include action".to_string())
        })?;
        let is_streaming = action == "streamGenerateContent";
        body.as_object_mut()
            .ok_or_else(|| {
                ProxyError::InvalidRequest("request body must be a JSON object".to_string())
            })?
            .insert("model".to_string(), Value::String(model.to_string()));
        return Ok((model.to_string(), is_streaming, body));
    }

    let model = body
        .get("model")
        .and_then(Value::as_str)
        .ok_or_else(|| ProxyError::InvalidRequest("missing model".to_string()))?
        .to_string();
    let is_streaming = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    Ok((model, is_streaming, body))
}

async fn write_execute_result(
    res: &mut Response,
    state: &AppState,
    request: ExecuteRequest,
    stream_context: StreamContext,
    result: crate::provider::executor::ExecuteResult,
    dump: Option<DebugDumpSession>,
) -> Result<(), ProxyError> {
    apply_headers(res, result.response.headers());
    res.status_code(
        StatusCode::from_u16(result.response.status())
            .map_err(|err| ProxyError::Upstream(format!("invalid upstream status: {err}")))?,
    );

    if !result.response.is_success() {
        write_passthrough_response(res, result.response, dump);
        return Ok(());
    }

    match result.response {
        UpstreamResponse::NonStream { body, .. } => {
            if result.provider_type == request.target {
                if let Some(session) = dump.as_ref() {
                    session.write_response_bytes(&body);
                }
                res.body(body);
                return Ok(());
            }

            // Dump unconverted upstream body before protocol conversion.
            if let Some(session) = dump.as_ref() {
                session.write_response_bytes(&body);
            }

            let raw_response_body = String::from_utf8_lossy(&body).to_string();
            let json = bytes_to_json(body).map_err(|error| {
                tracing::debug!(
                    source_provider = ?result.provider_type,
                    target_provider = ?request.target,
                    model = %request.model,
                    error = %error,
                    raw_response_body = %raw_response_body,
                    "response JSON parse failed"
                );
                error
            })?;
            let converted = state
                .providers
                .convert_response(result.provider_type, json, request.target)
                .map_err(|error| {
                    tracing::debug!(
                        source_provider = ?result.provider_type,
                        target_provider = ?request.target,
                        model = %request.model,
                        error = %error,
                        raw_response_body = %raw_response_body,
                        "response conversion failed"
                    );
                    error
                })?;
            res.render(Json(converted));
            Ok(())
        }
        UpstreamResponse::Stream { response, .. } => {
            if result.provider_type == request.target {
                let stream = response.bytes_stream().map_err(std::io::Error::other);
                res.stream(tee_stream(stream, dump));
                return Ok(());
            }

            let converter = state.providers.stream_converter(
                result.provider_type,
                request.target,
                stream_context,
            );
            // converted_stream dumps raw upstream chunks; client still receives converted SSE.
            let stream = converted_stream(
                response,
                converter,
                result.provider_type,
                request.target,
                request.model,
                dump,
            );
            res.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/event-stream"),
            );
            res.stream(stream);
            Ok(())
        }
    }
}

fn write_passthrough_response(
    res: &mut Response,
    response: UpstreamResponse,
    dump: Option<DebugDumpSession>,
) {
    match response {
        UpstreamResponse::NonStream { body, .. } => {
            if let Some(session) = dump.as_ref() {
                session.write_response_bytes(&body);
            }
            res.body(body);
        }
        UpstreamResponse::Stream { response, .. } => {
            let stream = response.bytes_stream().map_err(std::io::Error::other);
            res.stream(tee_stream(stream, dump));
        }
    }
}

fn converted_stream(
    response: reqwest::Response,
    converter: crate::stream::convert::StreamConverterImpl,
    source_provider: ProviderType,
    target_provider: ProviderType,
    model: String,
    dump: Option<DebugDumpSession>,
) -> impl futures_util::Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static {
    let model = std::sync::Arc::<str>::from(model);
    let dump = dump.map(std::sync::Arc::new);
    let upstream = response.bytes_stream();
    stream::unfold(
        (
            upstream,
            SseParser::default(),
            converter,
            Vec::<Bytes>::new(),
            false,
        ),
        move |(mut upstream, mut parser, mut converter, mut pending, mut finished)| {
            let model = model.clone();
            let dump = dump.clone();
            async move {
                if let Some(bytes) = pending.pop() {
                    return Some((Ok(bytes), (upstream, parser, converter, pending, finished)));
                }
                if finished {
                    return None;
                }

                loop {
                    match upstream.next().await {
                        Some(Ok(bytes)) => {
                            // Persist unconverted upstream bytes before protocol conversion.
                            if let Some(session) = dump.as_ref() {
                                session.append_response_chunk(&bytes);
                            }
                            match parser.push(&bytes) {
                                Ok(events) => {
                                    tracing::debug!(
                                        source_provider = ?source_provider,
                                        target_provider = ?target_provider,
                                        model = %model.as_ref(),
                                        raw_response_chunk = %String::from_utf8_lossy(&bytes),
                                        "upstream raw response chunk"
                                    );
                                    match convert_events(&mut converter, events) {
                                        Ok(mut out) => {
                                            out.reverse();
                                            if let Some(bytes) = out.pop() {
                                                return Some((
                                                    Ok(bytes),
                                                    (upstream, parser, converter, out, finished),
                                                ));
                                            }
                                        }
                                        Err(error) => {
                                            tracing::warn!(
                                                source_provider = ?source_provider,
                                                target_provider = ?target_provider,
                                                model = %model.as_ref(),
                                                error = %error,
                                                "stream response conversion failed"
                                            );
                                            return Some((
                                                Err(std::io::Error::other(error)),
                                                (upstream, parser, converter, pending, true),
                                            ));
                                        }
                                    }
                                }
                                Err(error) => {
                                    tracing::warn!(
                                        source_provider = ?source_provider,
                                        target_provider = ?target_provider,
                                        model = %model.as_ref(),
                                        error = %error,
                                        raw_response_chunk = %String::from_utf8_lossy(&bytes),
                                        "stream response parse failed"
                                    );
                                    return Some((
                                        Err(std::io::Error::other(error)),
                                        (upstream, parser, converter, pending, true),
                                    ));
                                }
                            }
                        }
                        Some(Err(error)) => {
                            tracing::warn!(
                                source_provider = ?source_provider,
                                target_provider = ?target_provider,
                                model = %model.as_ref(),
                                error = %error,
                                "stream upstream read failed"
                            );
                            return Some((
                                Err(std::io::Error::other(error)),
                                (upstream, parser, converter, pending, true),
                            ));
                        }
                        None => {
                            finished = true;
                            match parser.finish() {
                                Ok(Some(event)) => match converter.convert_event(event) {
                                    Ok(events) => {
                                        let mut out: Vec<Bytes> = events
                                            .into_iter()
                                            .map(|event| {
                                                encode_sse(event.event.as_deref(), &event.data)
                                            })
                                            .collect();
                                        out.reverse();
                                        if let Some(bytes) = out.pop() {
                                            return Some((
                                                Ok(bytes),
                                                (upstream, parser, converter, out, finished),
                                            ));
                                        }
                                    }
                                    Err(error) => {
                                        tracing::warn!(
                                            source_provider = ?source_provider,
                                            target_provider = ?target_provider,
                                            model = %model.as_ref(),
                                            error = %error,
                                            "stream response conversion failed"
                                        );
                                        return Some((
                                            Err(std::io::Error::other(error)),
                                            (upstream, parser, converter, pending, true),
                                        ));
                                    }
                                },
                                Ok(None) => return None,
                                Err(error) => {
                                    tracing::warn!(
                                        source_provider = ?source_provider,
                                        target_provider = ?target_provider,
                                        model = %model.as_ref(),
                                        error = %error,
                                        "stream response parse failed"
                                    );
                                    return Some((
                                        Err(std::io::Error::other(error)),
                                        (upstream, parser, converter, pending, true),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        },
    )
}

fn convert_events(
    converter: &mut crate::stream::convert::StreamConverterImpl,
    events: Vec<crate::stream::sse::SseEvent>,
) -> Result<Vec<Bytes>, ProxyError> {
    let mut out = Vec::new();
    for event in events {
        for event in converter.convert_event(event)? {
            out.push(encode_sse(event.event.as_deref(), &event.data));
        }
    }
    Ok(out)
}
