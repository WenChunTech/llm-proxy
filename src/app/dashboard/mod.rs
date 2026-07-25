mod auth_helpers;
mod config;
mod endpoints;
mod logs;
mod provider_models;
mod provider_test;
mod types;
mod validation;
mod validation_ws;

pub use endpoints::{build_provider_models_endpoint, build_provider_responses_endpoint};
pub(super) use logs::{
    api_debug_dump_delete, api_debug_dump_detail, api_debug_dump_file, api_debug_dumps,
    api_debug_dumps_delete, api_logs_snapshot, api_logs_ws,
};
pub use types::DashboardAuthProvider;
pub use validation::validation_auth_base_url;
pub(super) use validation_ws::{api_validate_codex_auths_ws, api_validate_grok_auths_ws};

use salvo::prelude::*;
use serde_json::json;

use crate::{error::ProxyError, provider::types::ProviderType};

use super::{JSON_MAX_SIZE, render_error, state_from_depot};
use config::config_payload;
pub(crate) use config::models_payload;
use provider_models::fetch_provider_models;
use provider_test::{
    stream_provider_model, test_provider_model, write_provider_test_stream_response,
};
use types::{AuthValidateRequest, DashboardConfig, ProviderModelsRequest, ProviderTestRequest};
use validation::validate_auths;

#[handler]
pub(super) async fn api_health(depot: &mut Depot, res: &mut Response) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let snapshot = state.snapshot().await;
    res.render(Json(json!({
        "status": "ok",
        "port": snapshot.config.port,
        "bind": snapshot.config.bind_addr(),
        "configured_models": snapshot.registry.configured_models().len(),
    })));
}

#[handler]
pub(super) async fn api_config(depot: &mut Depot, res: &mut Response) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let snapshot = state.snapshot().await;
    res.render(Json(config_payload(&snapshot)));
}

#[handler]
pub(super) async fn api_update_config(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let payload = match req
        .parse_json_with_max_size::<DashboardConfig>(JSON_MAX_SIZE)
        .await
    {
        Ok(payload) => payload,
        Err(error) => {
            render_error(
                res,
                ProxyError::InvalidRequest(format!("invalid JSON body: {error}")),
            );
            return;
        }
    };

    let current = state.snapshot().await.config.as_ref().clone();
    let next = payload.apply_to(current);
    match state.update_config(next).await {
        Ok(()) => {
            let snapshot = state.snapshot().await;
            res.render(Json(config_payload(&snapshot)));
        }
        Err(error) => render_error(res, error),
    }
}

#[handler]
pub(super) async fn api_models(depot: &mut Depot, res: &mut Response) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let snapshot = state.snapshot().await;
    res.render(Json(models_payload(&snapshot)));
}

#[handler]
pub(super) async fn api_provider_models(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let payload = match req
        .parse_json_with_max_size::<ProviderModelsRequest>(JSON_MAX_SIZE)
        .await
    {
        Ok(payload) => payload,
        Err(error) => {
            render_error(
                res,
                ProxyError::InvalidRequest(format!("invalid JSON body: {error}")),
            );
            return;
        }
    };

    match fetch_provider_models(&state.http, &payload).await {
        Ok(result) => res.render(Json(json!({
            "object": "list",
            "endpoint": result.endpoint,
            "data": result.model_ids
        }))),
        Err(error) => render_error(res, error),
    }
}

#[handler]
pub(super) async fn api_provider_test(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let payload = match req
        .parse_json_with_max_size::<ProviderTestRequest>(JSON_MAX_SIZE)
        .await
    {
        Ok(payload) => payload,
        Err(error) => {
            render_error(
                res,
                ProxyError::InvalidRequest(format!("invalid JSON body: {error}")),
            );
            return;
        }
    };

    if payload.stream {
        match stream_provider_model(&state, payload).await {
            Ok(response) => write_provider_test_stream_response(res, response),
            Err(error) => render_error(res, error),
        }
        return;
    }

    match test_provider_model(&state, payload).await {
        Ok(result) => res.render(Json(result)),
        Err(error) => render_error(res, error),
    }
}

#[handler]
pub(super) async fn api_validate_codex_auths(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let payload = match req
        .parse_json_with_max_size::<AuthValidateRequest>(JSON_MAX_SIZE)
        .await
    {
        Ok(payload) => payload,
        Err(error) => {
            render_error(
                res,
                ProxyError::InvalidRequest(format!("invalid JSON body: {error}")),
            );
            return;
        }
    };
    match validate_auths(&state, payload, ProviderType::Codex).await {
        Ok(result) => res.render(Json(result)),
        Err(error) => render_error(res, error),
    }
}

#[handler]
pub(super) async fn api_validate_grok_auths(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let Some(state) = state_from_depot(depot).ok() else {
        render_error(res, ProxyError::Config("missing app state".to_string()));
        return;
    };
    let payload = match req
        .parse_json_with_max_size::<AuthValidateRequest>(JSON_MAX_SIZE)
        .await
    {
        Ok(payload) => payload,
        Err(error) => {
            render_error(
                res,
                ProxyError::InvalidRequest(format!("invalid JSON body: {error}")),
            );
            return;
        }
    };
    match validate_auths(&state, payload, ProviderType::Grok).await {
        Ok(result) => res.render(Json(result)),
        Err(error) => render_error(res, error),
    }
}
