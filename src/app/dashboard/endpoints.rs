use crate::{
    error::ProxyError,
    provider::{oauth, types::ProviderType},
    util::append_url_path,
};

pub fn build_provider_models_endpoint(
    base_url: &str,
    provider_kind: &str,
) -> Result<String, ProxyError> {
    let mut url = reqwest::Url::parse(base_url)
        .map_err(|_| ProxyError::InvalidRequest("invalid base_url".to_string()))?;
    let pathname = url.path().trim_end_matches('/').to_string();

    if pathname.ends_with("/models") {
        url.set_path(if pathname.is_empty() {
            "/models"
        } else {
            &pathname
        });
    } else {
        let is_openai_style = ProviderType::from_config_id(provider_kind)
            .is_some_and(ProviderType::uses_openai_models_endpoint);
        let has_version_path = has_version_path(&pathname);
        let suffix = if !is_openai_style && !has_version_path {
            "v1/models"
        } else {
            "models"
        };
        url.set_path(&append_url_path(&pathname, suffix));
    }

    Ok(url.to_string())
}

pub fn build_provider_responses_endpoint(base_url: &str) -> Result<String, ProxyError> {
    oauth::responses_endpoint(base_url)
}

fn has_version_path(pathname: &str) -> bool {
    let Some(segment) = pathname.rsplit('/').find(|segment| !segment.is_empty()) else {
        return false;
    };
    let Some(version) = segment.strip_prefix('v') else {
        return false;
    };
    !version.is_empty() && version.chars().all(|item| item.is_ascii_digit())
}
