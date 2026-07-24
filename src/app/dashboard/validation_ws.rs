//! WebSocket streaming for Codex/Grok auth validation progress.

use futures_util::StreamExt;
use salvo::prelude::*;
use salvo::websocket::{Message, WebSocketUpgrade};
use tokio::sync::mpsc;

use crate::{provider::types::ProviderType, state::AppState};

use super::{
    render_error, state_from_depot,
    types::AuthValidateRequest,
    validation::{AuthValidateStreamEvent, validate_auths_with_progress},
};

#[handler]
pub(in crate::app) async fn api_validate_codex_auths_ws(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<(), StatusError> {
    upgrade_validate_ws(req, depot, res, ProviderType::Codex).await
}

#[handler]
pub(in crate::app) async fn api_validate_grok_auths_ws(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<(), StatusError> {
    upgrade_validate_ws(req, depot, res, ProviderType::Grok).await
}

async fn upgrade_validate_ws(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
    provider_type: ProviderType,
) -> Result<(), StatusError> {
    let state = match state_from_depot(depot) {
        Ok(state) => state,
        Err(error) => {
            render_error(res, error);
            return Err(StatusError::internal_server_error());
        }
    };

    WebSocketUpgrade::new()
        .upgrade(req, res, move |ws| {
            handle_validate_socket(ws, state, provider_type)
        })
        .await
}

async fn handle_validate_socket(
    mut ws: salvo::websocket::WebSocket,
    state: AppState,
    provider_type: ProviderType,
) {
    let request = match recv_validate_request(&mut ws).await {
        Ok(request) => request,
        Err(message) => {
            let _ = send_event(&mut ws, &AuthValidateStreamEvent::Error { message }).await;
            return;
        }
    };

    let (tx, mut rx) = mpsc::channel::<AuthValidateStreamEvent>(64);
    let worker = tokio::spawn(async move {
        validate_auths_with_progress(&state, request, provider_type, Some(tx)).await
    });

    while let Some(event) = rx.recv().await {
        if send_event(&mut ws, &event).await.is_err() {
            // Client disconnected; let the worker finish (or drop the channel).
            break;
        }
    }

    match worker.await {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => {
            let _ = send_event(
                &mut ws,
                &AuthValidateStreamEvent::Error {
                    message: error.to_string(),
                },
            )
            .await;
        }
        Err(error) => {
            let _ = send_event(
                &mut ws,
                &AuthValidateStreamEvent::Error {
                    message: format!("validation task failed: {error}"),
                },
            )
            .await;
        }
    }
}

async fn recv_validate_request(
    ws: &mut salvo::websocket::WebSocket,
) -> Result<AuthValidateRequest, String> {
    loop {
        match ws.next().await {
            Some(Ok(msg)) if msg.is_close() => {
                return Err("websocket closed before request".to_string());
            }
            Some(Ok(msg)) if msg.is_text() => {
                let text = msg
                    .as_str()
                    .map_err(|error| format!("invalid websocket text frame: {error}"))?;
                return serde_json::from_str(text)
                    .map_err(|error| format!("invalid validation request JSON: {error}"));
            }
            Some(Ok(_)) => continue,
            Some(Err(error)) => {
                return Err(format!("websocket receive failed: {error}"));
            }
            None => return Err("websocket closed before request".to_string()),
        }
    }
}

async fn send_event(
    ws: &mut salvo::websocket::WebSocket,
    event: &AuthValidateStreamEvent,
) -> Result<(), ()> {
    let text = serde_json::to_string(event).map_err(|_| ())?;
    ws.send(Message::text(text)).await.map_err(|_| ())
}
