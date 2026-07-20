use salvo::prelude::*;

use crate::state::AppState;

#[handler]
pub async fn auth(req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
    if req.method() == salvo::http::Method::OPTIONS {
        ctrl.call_next(req, depot, res).await;
        return;
    }

    let Some(state) = depot.get::<AppState>("state").ok().cloned() else {
        res.status_code(salvo::http::StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(
            crate::error::ProxyError::Config("missing app state".to_string()).error_body(),
        ));
        return;
    };

    let snapshot = state.snapshot().await;
    let Some(configured) = snapshot
        .config
        .api_key
        .as_ref()
        .map(|key| key.trim())
        .filter(|key| !key.is_empty())
    else {
        ctrl.call_next(req, depot, res).await;
        return;
    };

    let request_key = request_api_key(req);
    if request_key == configured {
        ctrl.call_next(req, depot, res).await;
        return;
    }

    tracing::warn!(
        method = %req.method(),
        path = %req.uri().path(),
        "unauthorized request"
    );
    let error = crate::error::ProxyError::Unauthorized;
    res.status_code(error.status_code());
    res.render(Json(error.error_body()));
}

fn request_api_key(req: &Request) -> String {
    if let Some(value) = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
    {
        let mut parts = value.trim().splitn(2, char::is_whitespace);
        if let (Some(scheme), Some(token)) = (parts.next(), parts.next())
            && scheme.eq_ignore_ascii_case("bearer")
            && !token.trim().is_empty()
        {
            return token.trim().to_string();
        }
    }

    for name in ["x-api-key", "x-goog-api-key"] {
        if let Some(value) = req.headers().get(name).and_then(|v| v.to_str().ok()) {
            return value.to_string();
        }
    }
    String::new()
}
