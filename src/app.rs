pub mod dashboard;
mod frontend;
mod proxy;

use salvo::{
    http::{HeaderName, HeaderValue, StatusCode, header},
    prelude::*,
};
use serde_json::json;

use crate::{
    error::ProxyError,
    middleware::{auth::auth, headers::filter_response_headers},
    state::AppState,
};

pub(crate) const JSON_MAX_SIZE: usize = usize::MAX;

pub fn router(state: AppState) -> Router {
    Router::new()
        .hoop(StateHoop { state })
        .hoop(cors)
        .push(Router::with_path("health").get(health))
        .push(
            Router::with_path("api")
                .hoop(auth)
                .push(Router::with_path("health").get(dashboard::api_health))
                .push(
                    Router::with_path("config")
                        .get(dashboard::api_config)
                        .put(dashboard::api_update_config),
                )
                .push(Router::with_path("models").get(dashboard::api_models))
                .push(Router::with_path("provider-models").post(dashboard::api_provider_models))
                .push(Router::with_path("codex/validate").post(dashboard::api_validate_codex_auths))
                .push(Router::with_path("grok/validate").post(dashboard::api_validate_grok_auths))
                .push(
                    Router::with_path("settings/codex/validate")
                        .post(dashboard::api_validate_codex_auths),
                )
                .push(
                    Router::with_path("settings/grok/validate")
                        .post(dashboard::api_validate_grok_auths),
                )
                .push(Router::with_path("provider-test").post(dashboard::api_provider_test)),
        )
        .push(
            Router::with_path("v1")
                .hoop(auth)
                .push(Router::with_path("chat/completions").post(proxy::openai_chat))
                .push(Router::with_path("images/generations").post(proxy::image_generations))
                .push(Router::with_path("responses").post(proxy::openai_responses))
                .push(Router::with_path("messages").post(proxy::claude_messages))
                .push(Router::with_path("models").get(proxy::models)),
        )
        .push(
            Router::with_path("v1beta")
                .hoop(auth)
                .push(Router::with_path("models/{modelName}").post(proxy::gemini_model)),
        )
        .push(Router::with_path("{**path}").options(cors_preflight))
        .push(Router::with_path("{**path}").get(frontend::frontend_asset))
}

#[derive(Clone)]
struct StateHoop {
    state: AppState,
}

#[handler]
impl StateHoop {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        depot.insert("state", self.state.clone());
        ctrl.call_next(req, depot, res).await;
    }
}

#[handler]
async fn cors(req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
    res.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    res.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("POST, GET, PUT, OPTIONS"),
    );
    res.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Content-Type, Authorization, x-api-key, x-goog-api-key"),
    );

    if req.method() == salvo::http::Method::OPTIONS {
        res.status_code(StatusCode::NO_CONTENT);
        return;
    }
    ctrl.call_next(req, depot, res).await;
}

#[handler]
async fn cors_preflight(res: &mut Response) {
    res.status_code(StatusCode::NO_CONTENT);
}

#[handler]
async fn health(res: &mut Response) {
    res.render(Json(json!({"status": "ok"})));
}

pub(crate) fn state_from_depot(depot: &mut Depot) -> Result<AppState, ProxyError> {
    depot
        .get::<AppState>("state")
        .ok()
        .cloned()
        .ok_or_else(|| ProxyError::Config("missing app state".to_string()))
}

pub(crate) fn apply_headers(res: &mut Response, headers: &crate::provider::types::HeaderMap) {
    for (name, value) in filter_response_headers(headers) {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(&value),
        ) {
            res.headers_mut().insert(name, value);
        }
    }
}

pub(crate) fn render_error(res: &mut Response, error: ProxyError) {
    let status_code = error.status_code();
    let upstream_status_code = error.upstream_status_code().map(|status| status.as_u16());
    let upstream_url = error.upstream_url();
    tracing::warn!(
        error = %error,
        status_code = status_code.as_u16(),
        upstream_status_code = ?upstream_status_code,
        upstream_url = upstream_url.as_deref().unwrap_or(""),
        "request failed"
    );
    res.status_code(status_code);
    res.render(Json(error.error_body()));
}
