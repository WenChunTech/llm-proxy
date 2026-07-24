//! EnvFilter helpers for quiet dependency logs (h2, hyper, etc.).

use std::sync::OnceLock;

use tracing_subscriber::{EnvFilter, Registry, reload};

/// Crates/targets that should follow the configured application log level.
const APP_TARGETS: &[&str] = &["llm_proxy", "converter", "slave"];

type FilterReloadHandle = reload::Handle<EnvFilter, Registry>;

static FILTER_RELOAD: OnceLock<FilterReloadHandle> = OnceLock::new();

/// Install the reload handle created during `init_tracing`.
pub fn install_reload_handle(handle: FilterReloadHandle) {
    let _ = FILTER_RELOAD.set(handle);
}

/// Resolve the runtime filter.
///
/// Priority:
/// 1. `RUST_LOG` when set (full manual control at startup)
/// 2. `log_level` from config
///
/// A bare level such as `debug` / `info` is expanded to only enable this
/// project and selected targets, keeping noisy dependencies (e.g. `h2`) at
/// `warn` or above. Full EnvFilter directives (`a=b,c=d`) are used as-is.
pub fn resolve_env_filter(
    log_level: Option<&str>,
) -> Result<EnvFilter, tracing_subscriber::filter::ParseError> {
    match EnvFilter::try_from_default_env() {
        Ok(filter) => Ok(filter),
        Err(_) => EnvFilter::try_new(expand_log_level(log_level.unwrap_or("info"))),
    }
}

/// Build an EnvFilter from a config `log_level` value (always expanded / parsed).
pub fn filter_from_log_level(
    log_level: Option<&str>,
) -> Result<EnvFilter, tracing_subscriber::filter::ParseError> {
    EnvFilter::try_new(expand_log_level(log_level.unwrap_or("info")))
}

/// Hot-reload the process filter after a dashboard config change.
///
/// When the reload handle is missing (e.g. unit tests), this is a no-op.
pub fn apply_log_level(log_level: Option<&str>) -> Result<(), String> {
    let Some(handle) = FILTER_RELOAD.get() else {
        return Ok(());
    };
    let filter = filter_from_log_level(log_level).map_err(|error| error.to_string())?;
    handle
        .reload(filter)
        .map_err(|error| format!("reload log filter failed: {error}"))
}

/// Expand a config `log_level` value into an EnvFilter directive string.
pub fn expand_log_level(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return app_scoped_filter("info");
    }
    // Already a full directive list / target=level expression.
    if raw.contains('=') || raw.contains(',') {
        return raw.to_string();
    }
    app_scoped_filter(raw)
}

fn app_scoped_filter(level: &str) -> String {
    // Default baseline `warn` silences info/debug chatter from h2, hyper,
    // reqwest, rustls, tokio, etc. App targets override to the requested level.
    let mut directive = String::from("warn");
    for target in APP_TARGETS {
        directive.push(',');
        directive.push_str(target);
        directive.push('=');
        directive.push_str(level);
    }
    directive
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_level_scopes_to_app_targets() {
        assert_eq!(
            expand_log_level("debug"),
            "warn,llm_proxy=debug,converter=debug,slave=debug"
        );
        assert_eq!(
            expand_log_level("info"),
            "warn,llm_proxy=info,converter=info,slave=info"
        );
    }

    #[test]
    fn full_directive_is_preserved() {
        assert_eq!(expand_log_level("debug,h2=trace"), "debug,h2=trace");
        assert_eq!(
            expand_log_level("llm_proxy=trace,h2=off"),
            "llm_proxy=trace,h2=off"
        );
    }

    #[test]
    fn empty_falls_back_to_info_scope() {
        assert_eq!(
            expand_log_level("  "),
            "warn,llm_proxy=info,converter=info,slave=info"
        );
    }

    #[test]
    fn expanded_directive_parses() {
        EnvFilter::try_new(expand_log_level("debug")).unwrap();
        EnvFilter::try_new(expand_log_level("llm_proxy=debug,h2=off")).unwrap();
    }

    #[test]
    fn apply_without_handle_is_ok() {
        apply_log_level(Some("info")).unwrap();
    }
}
