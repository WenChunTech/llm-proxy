mod auth;
mod config;
mod log;
mod request;

use eventsource_stream::Eventsource;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::LazyLock;
use tokio::sync::mpsc;
use tokio_stream::{StreamExt, wrappers::UnboundedReceiverStream};
use tracing::error;
use tracing::info;
use tracing::instrument;
use warp::{Filter, Reply, http::StatusCode, sse::Event};

use tokio::sync::Mutex;

static CONFIG: LazyLock<Mutex<config::Config>> =
    LazyLock::new(|| Mutex::new(config::load_config()));

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
    let gemini_req: converter::models::gemini::Request = req_body.into();
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
            let stream = resp.bytes_stream().eventsource();
            let event_stream = stream.map(|thing| {
                let event = match thing {
                    Ok(event) => {
                        if event.data == "[DONE]" {
                            Event::default().data("[DONE]")
                        } else {
                            match serde_json::from_str::<converter::models::gemini::Response>(
                                &event.data,
                            ) {
                                Ok(mut data) => {
                                    data.response.extension.insert(
                                        "stream".to_string(),
                                        serde_json::Value::Bool(true),
                                    );
                                    info!("gemini response: {}", event.data);
                                    let openai_resp: converter::models::openai::Response =
                                        data.into();
                                    match serde_json::to_string(&openai_resp) {
                                        Ok(openai_resp_str) => {
                                            info!("openai response: {}", openai_resp_str);
                                            Event::default().data(openai_resp_str)
                                        }
                                        Err(e) => {
                                            error!("openai response serialization error: {}", e);
                                            Event::default().event("error").data(e.to_string())
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!(
                                        "gemini response deserialization error: {}. data: {}",
                                        e, event.data
                                    );
                                    Event::default().event("error").data(e.to_string())
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("eventsource error: {}", e);
                        Event::default().event("error").data(e.to_string())
                    }
                };
                Ok::<_, Infallible>(event)
            });
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

#[instrument]
async fn handle_claude(
    body: warp::hyper::body::Bytes,
    // mut req_body: converter::models::claude::Request,
) -> Result<impl warp::Reply, Infallible> {
    let raw = &String::from_utf8(body.to_vec()).unwrap();
    info!(claude.request = raw);
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
    req_body.extension = extension;
    if req_body.model == "claude-3-5-haiku-20241022" {
        req_body.model = "gemini-2.5-flash".to_string();
    }
    let openai_req: converter::models::openai::Request = req_body.into();
    info!(openai.request = serde_json::to_string(&openai_req).unwrap());
    let gemini_req: converter::models::gemini::Request = openai_req.into();
    info!(gemini.request = serde_json::to_string(&gemini_req).unwrap());
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
                let mut openai_chunks = Vec::new();
                let mut stream = std::pin::pin!(stream);

                while let Some(event) = stream.next().await {
                    match event {
                        Ok(event) => {
                            if event.data == "[DONE]" {
                                break;
                            }
                            match serde_json::from_str::<converter::models::gemini::Response>(
                                &event.data,
                            ) {
                                Ok(gemini_resp) => {
                                    info!(gemini.response = &event.data);
                                    let openai_resp: converter::models::openai::Response =
                                        gemini_resp.into();
                                    info!(
                                        openai.response =
                                            serde_json::to_string(&openai_resp).unwrap()
                                    );
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

                let claude_events =
                    converter::convert::claude::stream_response::convert_openai_to_claude_stream(
                        openai_chunks,
                    );

                for event in claude_events {
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

async fn handle_gemini(
    _req_body: converter::models::gemini::Request,
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
        .and_then(handle_openai);
    let claude = warp::path!("v1" / "messages")
        .and(warp::post())
        .and(warp::body::bytes())
        .and_then(handle_claude);
    let gemini = warp::path!("v1internal:streamGenerateContent")
        .and(warp::post())
        .and(warp::body::json())
        .and_then(handle_gemini);
    let routes = openai.or(claude).or(gemini);

    let addr: SocketAddr = {
        let config = CONFIG.lock().await;
        format!("{}:{}", config.host, config.port).parse().unwrap()
    };
    warp::serve(routes).run(addr).await;
}
