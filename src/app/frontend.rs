use bytes::Bytes;
use rust_embed::RustEmbed;
use salvo::{
    http::{HeaderValue, StatusCode, header},
    prelude::*,
};

#[derive(RustEmbed)]
#[folder = "frontend/dist"]
struct FrontendAssets;

#[handler]
pub(super) async fn frontend_asset(req: &mut Request, res: &mut Response) {
    let raw_path = req.param::<String>("path").unwrap_or_default();
    let path = normalize_asset_path(&raw_path);
    let asset = FrontendAssets::get(&path).or_else(|| FrontendAssets::get("index.html"));
    let Some(asset) = asset else {
        res.status_code(StatusCode::NOT_FOUND);
        return;
    };
    let content_type = mime_guess::from_path(&path)
        .first_or_octet_stream()
        .as_ref()
        .to_string();
    if let Ok(value) = HeaderValue::from_str(&content_type) {
        res.headers_mut().insert(header::CONTENT_TYPE, value);
    }
    res.body(Bytes::from(asset.data.into_owned()));
}

fn normalize_asset_path(path: &str) -> String {
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        "index.html".to_string()
    } else {
        path.to_string()
    }
}
