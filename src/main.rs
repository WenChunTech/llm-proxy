mod auth;
mod config;
mod log;
mod request;

use converter::convert::Inner;
use converter::convert::claude;
use converter::convert::claude::ClaudeStreamResponseWrapper;
use converter::convert::gemini_cli;
use converter::convert::gemini_cli::OpenAIStreamResponseWrapper;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::LazyLock;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::error;
use tracing::info;
use warp::{Filter, Reply, http::StatusCode, sse::Event};

use tokio::sync::Mutex;

static CONFIG: LazyLock<Mutex<config::Config>> =
    LazyLock::new(|| Mutex::new(config::load_config()));

async fn handle_openai_iflow(
    req_body: converter::models::openai::Request,
) -> Result<impl warp::Reply, Infallible> {
    let resp = reqwest::Client::new()
        .post("https://apis.iflow.cn/v1/chat/completions")
        .bearer_auth("sk-c7b4208c9b777de19ca8f8628f6845b2")
        .json(&req_body)
        .send()
        .await;
    let reply = match resp {
        Ok(resp) => {
            if !resp.status().is_success() {
                return Ok(warp::reply::with_status(
                    warp::reply::html(resp.text().await.unwrap_or_default()),
                    StatusCode::INTERNAL_SERVER_ERROR,
                )
                .into_response());
            }
            let (tx, rx) = mpsc::unbounded_channel::<Result<warp::sse::Event, Infallible>>();
            tokio::spawn(async move {
                let stream = resp.bytes_stream().eventsource();
                let mut stream = std::pin::pin!(stream);
                while let Some(event) = stream.next().await {
                    match event {
                        Ok(event) => {
                            match serde_json::from_str::<converter::models::openai::Response>(
                                &event.data,
                            ) {
                                Ok(openai_resp) => {
                                    info!(openai.response = &event.data);
                                    let data = serde_json::to_string(&openai_resp).unwrap();
                                    if let Err(e) = tx.send(Ok(Event::default().data(data))) {
                                        error!("Failed to send event to channel: {}", e);
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to parse gemini response: {}", e);
                                }
                            }
                        }
                        Err(e) => error!("Stream error: {}", e),
                    }
                }
            });
            let event_stream = UnboundedReceiverStream::new(rx);
            warp::sse::reply(event_stream).into_response()
        }
        Err(e) => {
            error!("gemini response Error: {}", e);
            warp::reply::with_status(
                warp::reply::html(e.to_string()),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
            .into_response()
        }
    };
    Ok(reply)
}

async fn handle_openai(
    mut req_body: converter::models::openai::Request,
) -> Result<impl warp::Reply, Infallible> {
    let mut config = CONFIG.lock().await;
    let mut extension = HashMap::new();
    let project_id = config
        .gemini
        .as_ref()
        .unwrap()
        .first()
        .unwrap()
        .project_id
        .clone();
    extension.insert(
        "project_id".to_string(),
        serde_json::Value::String(project_id),
    );
    req_body.extension = extension;
    let model = req_body.model.clone();
    let gemini_req: converter::models::gemini_cli::Request = req_body.into();
    let token = auth::gemini::token(&mut config).await.unwrap();
    let ak = token.access_token;
    let resp = request::gemini::gemini_request(gemini_req, &ak).await;
    let reply = match resp {
        Ok(resp) => {
            if !resp.status().is_success() {
                return Ok(warp::reply::with_status(
                    warp::reply::html(resp.text().await.unwrap_or_default()),
                    StatusCode::INTERNAL_SERVER_ERROR,
                )
                .into_response());
            }
            let (tx, rx) =
                tokio::sync::mpsc::unbounded_channel::<Result<warp::sse::Event, Infallible>>();
            tokio::spawn(async move {
                let stream = resp.bytes_stream().eventsource();
                let mut stream = std::pin::pin!(stream);

                let mut inner = Inner::default();
                while let Some(event) = stream.next().await {
                    match event {
                        Ok(event) => {
                            if let Ok(mut gemini_resp) = serde_json::from_str::<
                                converter::models::gemini_cli::Response,
                            >(&event.data)
                            {
                                gemini_resp.extension.insert(
                                    "model".to_string(),
                                    serde_json::Value::String(model.clone()),
                                );
                                info!(gemini.response = &event.data);
                                let gemini_cli_wrapper = gemini_cli::GeminiCLiStreamWrapper {
                                    chunk: gemini_resp,
                                    inner,
                                };
                                let openai_wrapper = gemini_cli_wrapper.convert();
                                inner = openai_wrapper.inner;
                                for openai_chunk in openai_wrapper.chunk {
                                    info!(
                                        openai.response =
                                            serde_json::to_string(&openai_chunk).unwrap()
                                    );
                                    let data = serde_json::to_string(&openai_chunk).unwrap();
                                    if let Err(e) = tx.send(Ok(Event::default().data(data))) {
                                        error!("Failed to send event to channel: {}", e);
                                    }
                                }
                            }
                        }
                        Err(e) => error!("Stream error: {}", e),
                    }
                }
            });

            let event_stream = UnboundedReceiverStream::new(rx);
            warp::sse::reply(event_stream).into_response()
        }
        Err(e) => {
            error!("gemini response Error: {}", e);
            warp::reply::with_status(
                warp::reply::html(e.to_string()),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
            .into_response()
        }
    };
    Ok(reply)
}

async fn handle_claude_for_flow(
    body: warp::hyper::body::Bytes,
) -> Result<impl warp::Reply, Infallible> {
    let mut req_body: converter::models::claude::Request = serde_json::from_slice(&body).unwrap();
    let mut extension = HashMap::new();
    extension.insert("chat_id".to_string(), serde_json::Value::Null);
    req_body.extension = extension;
    // if req_body.model == "claude-3-5-haiku-20241022" {
    //     req_body.model = "gemini-2.5-flash".to_string();
    // }
    let model = req_body.model.clone();
    let openai_req: converter::models::openai::Request = req_body.into();
    let resp = reqwest::Client::new()
        .post("https://apis.iflow.cn/v1/chat/completions")
        .bearer_auth("sk-fc668d2757bda624b8c54f1b9d983441")
        .json(&openai_req)
        .send()
        .await;

    let reply = match resp {
        Ok(resp) => {
            if !resp.status().is_success() {
                return Ok(warp::reply::with_status(
                    warp::reply::html(resp.text().await.unwrap_or_default()),
                    StatusCode::INTERNAL_SERVER_ERROR,
                )
                .into_response());
            }
            let (tx, rx) = mpsc::unbounded_channel::<Result<warp::sse::Event, Infallible>>();
            tokio::spawn(async move {
                let stream = resp.bytes_stream().eventsource();
                let mut openai_chunks = Vec::new();
                let mut stream = std::pin::pin!(stream);

                while let Some(event) = stream.next().await {
                    match event {
                        Ok(event) => {
                            match serde_json::from_str::<converter::models::openai::Response>(
                                &event.data,
                            ) {
                                Ok(mut openai_resp) => {
                                    openai_resp.extension.insert(
                                        "model".to_string(),
                                        serde_json::Value::String(model.clone()),
                                    );
                                    info!(gemini.response = &event.data);
                                    openai_chunks.push(openai_resp);
                                }
                                Err(e) => {
                                    error!("Failed to parse gemini response: {}", e);
                                }
                            }
                        }
                        Err(e) => error!("Stream error: {}", e),
                    }
                }
                for openai_chunk in &openai_chunks {
                    info!(openai.response = serde_json::to_string(openai_chunk).unwrap());
                }
                let claude_chunks: ClaudeStreamResponseWrapper = openai_chunks.into();
                for event in claude_chunks.0 {
                    let event_name = event.get_type();
                    let data = serde_json::to_string(&event).unwrap();
                    info!(claude.response = data);
                    if let Err(e) = tx.send(Ok(Event::default().event(event_name).data(data))) {
                        error!("Failed to send event to channel: {}", e);
                    }
                }
            });

            let event_stream = UnboundedReceiverStream::new(rx);
            warp::sse::reply(event_stream).into_response()
        }
        Err(e) => {
            error!("gemini response Error: {}", e);
            warp::reply::with_status(
                warp::reply::html(e.to_string()),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
            .into_response()
        }
    };
    Ok(reply)
}

async fn handle_claude_new(body: warp::hyper::body::Bytes) -> Result<impl warp::Reply, Infallible> {
    let mut req_body: converter::models::claude::Request = serde_json::from_slice(&body).unwrap();
    let mut config = CONFIG.lock().await;
    let mut extension = HashMap::new();
    let project_id = config
        .gemini
        .as_ref()
        .unwrap()
        .first()
        .unwrap()
        .project_id
        .clone();
    extension.insert(
        "project_id".to_string(),
        serde_json::Value::String(project_id),
    );
    extension.insert("chat_id".to_string(), serde_json::Value::Null);
    req_body.extension = extension;
    if req_body.model == "claude-3-5-haiku-20241022" {
        req_body.model = "gemini-2.5-flash".to_string();
    }
    let model = req_body.model.clone();
    let openai_req: converter::models::openai::Request = req_body.into();
    let gemini_req: converter::models::gemini_cli::Request = openai_req.into();
    let token = auth::gemini::token(&mut config).await.unwrap();
    let ak = token.access_token;
    let resp = request::gemini::gemini_request(gemini_req, &ak).await;

    let reply = match resp {
        Ok(resp) => {
            if !resp.status().is_success() {
                return Ok(warp::reply::with_status(
                    warp::reply::html(resp.text().await.unwrap_or_default()),
                    StatusCode::INTERNAL_SERVER_ERROR,
                )
                .into_response());
            }
            let (tx, rx) =
                tokio::sync::mpsc::unbounded_channel::<Result<warp::sse::Event, Infallible>>();
            tokio::spawn(async move {
                let stream = resp.bytes_stream().eventsource();
                let mut stream = std::pin::pin!(stream);

                let mut inner = Inner::default();
                while let Some(event) = stream.next().await {
                    match event {
                        Ok(event) => {
                            if let Ok(mut gemini_resp) = serde_json::from_str::<
                                converter::models::gemini_cli::Response,
                            >(&event.data)
                            {
                                gemini_resp.extension.insert(
                                    "model".to_string(),
                                    serde_json::Value::String(model.clone()),
                                );
                                info!(gemini.response = &event.data);
                                let gemini_cli_wrapper = gemini_cli::GeminiCLiStreamWrapper {
                                    chunk: gemini_resp,
                                    inner,
                                };
                                let openai_wrapper = gemini_cli_wrapper.convert();
                                inner = openai_wrapper.inner;
                                for openai_chunk in openai_wrapper.chunk {
                                    info!(
                                        openai.response =
                                            serde_json::to_string(&openai_chunk).unwrap()
                                    );
                                    let openai_wrapper = claude::OpenAIStreamWrapper {
                                        chunk: openai_chunk,
                                        inner,
                                    };
                                    let claude_wrapper = openai_wrapper.convert();
                                    inner = claude_wrapper.inner;
                                    for claude_chunk in claude_wrapper.chunks {
                                        info!(
                                            claude.response =
                                                serde_json::to_string(&claude_chunk).unwrap()
                                        );
                                        let event_name = claude_chunk.get_type();
                                        let data = serde_json::to_string(&claude_chunk).unwrap();
                                        if let Err(e) = tx
                                            .send(Ok(Event::default().event(event_name).data(data)))
                                        {
                                            error!("Failed to send event to channel: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => error!("Stream error: {}", e),
                    }
                }
            });

            let event_stream = UnboundedReceiverStream::new(rx);
            warp::sse::reply(event_stream).into_response()
        }
        Err(e) => {
            error!("gemini response Error: {}", e);
            warp::reply::with_status(
                warp::reply::html(e.to_string()),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
            .into_response()
        }
    };
    Ok(reply)
}

async fn handle_claude(
    body: warp::hyper::body::Bytes,
    // mut req_body: converter::models::claude::Request,
) -> Result<impl warp::Reply, Infallible> {
    // let raw = &String::from_utf8(body.to_vec()).unwrap();
    // info!(claude.request = raw);
    let mut req_body: converter::models::claude::Request = serde_json::from_slice(&body).unwrap();
    let mut config = CONFIG.lock().await;
    let mut extension = HashMap::new();
    let project_id = config
        .gemini
        .as_ref()
        .unwrap()
        .first()
        .unwrap()
        .project_id
        .clone();
    extension.insert(
        "project_id".to_string(),
        serde_json::Value::String(project_id),
    );
    extension.insert("chat_id".to_string(), serde_json::Value::Null);
    req_body.extension = extension;
    if req_body.model == "claude-3-5-haiku-20241022" {
        req_body.model = "gemini-2.5-flash".to_string();
    }
    let model = req_body.model.clone();
    let openai_req: converter::models::openai::Request = req_body.into();
    // info!(openai.request = serde_json::to_string(&openai_req).unwrap());
    let gemini_req: converter::models::gemini_cli::Request = openai_req.into();
    // info!(gemini.request = serde_json::to_string(&gemini_req).unwrap());
    let token = auth::gemini::token(&mut config).await.unwrap();
    let ak = token.access_token;
    let resp = request::gemini::gemini_request(gemini_req, &ak).await;

    let reply = match resp {
        Ok(resp) => {
            if !resp.status().is_success() {
                return Ok(warp::reply::with_status(
                    warp::reply::html(resp.text().await.unwrap_or_default()),
                    StatusCode::INTERNAL_SERVER_ERROR,
                )
                .into_response());
            }
            let (tx, rx) = mpsc::unbounded_channel::<Result<warp::sse::Event, Infallible>>();
            tokio::spawn(async move {
                let stream = resp.bytes_stream().eventsource();
                let mut gemini_cli_chunks = Vec::new();
                let mut stream = std::pin::pin!(stream);

                while let Some(event) = stream.next().await {
                    match event {
                        Ok(event) => {
                            match serde_json::from_str::<converter::models::gemini_cli::Response>(
                                &event.data,
                            ) {
                                Ok(mut gemini_resp) => {
                                    gemini_resp.extension.insert(
                                        "model".to_string(),
                                        serde_json::Value::String(model.clone()),
                                    );
                                    info!(gemini.response = &event.data);
                                    gemini_cli_chunks.push(gemini_resp);
                                }
                                Err(e) => {
                                    error!("Failed to parse gemini response: {}", e);
                                }
                            }
                        }
                        Err(e) => error!("Stream error: {}", e),
                    }
                }
                let openai_chunks_wrapper: OpenAIStreamResponseWrapper = gemini_cli_chunks.into();
                for chunk in &openai_chunks_wrapper.0 {
                    info!(openai.response = serde_json::to_string(chunk).unwrap());
                }
                let claude_chunks: ClaudeStreamResponseWrapper = openai_chunks_wrapper.0.into();
                for event in claude_chunks.0 {
                    let event_name = event.get_type();
                    let data = serde_json::to_string(&event).unwrap();
                    info!(claude.response = data);
                    if let Err(e) = tx.send(Ok(Event::default().event(event_name).data(data))) {
                        error!("Failed to send event to channel: {}", e);
                    }
                }
            });

            let event_stream = UnboundedReceiverStream::new(rx);
            warp::sse::reply(event_stream).into_response()
        }
        Err(e) => {
            error!("gemini response Error: {}", e);
            warp::reply::with_status(
                warp::reply::html(e.to_string()),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
            .into_response()
        }
    };
    Ok(reply)
}

async fn handle_gemini_cli(
    _req_body: converter::models::gemini_cli::Request,
) -> Result<impl warp::Reply, Infallible> {
    Ok("Hello from warp!")
}

#[tokio::main]
async fn main() {
    log::init_log();

    let _config = config::load_config();

    let openai = warp::path!("v1" / "chat" / "completions")
        .and(warp::post())
        .and(warp::body::json())
        // .and_then(handle_openai_iflow);
        .and_then(handle_openai);
    let claude = warp::path!("v1" / "messages")
        .and(warp::post())
        .and(warp::body::bytes())
        // .and_then(handle_claude_for_flow);
        .and_then(handle_claude_new);
    let gemini_cli = warp::path!("v1internal:streamGenerateContent")
        .and(warp::post())
        .and(warp::body::json())
        .and_then(handle_gemini_cli);
    // let gemini = warp::path!("v1beta" / "models" / String / "generateContent")
    //     .and(warp::post())
    //     .and(warp::body::json())
    //     .and_then(handle_gemini);

    let routes = openai.or(claude).or(gemini_cli);

    let addr: SocketAddr = {
        let config = CONFIG.lock().await;
        format!("{}:{}", config.host, config.port).parse().unwrap()
    };
    warp::serve(routes).run(addr).await;
}

// sk-fc668d2757bda624b8c54f1b9d983441
