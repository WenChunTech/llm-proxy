//! EnvFilter helpers for quiet dependency logs (h2, hyper, etc.).

use tracing_subscriber::EnvFilter;

/// Crates/targets that should follow the configured application log level.
const APP_TARGETS: &[&str] = &["llm_proxy", "converter", "slave"];

/// Resolve the runtime filter.
///
/// Priority:
/// 1. `RUST_LOG` when set (full manual control)
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
        assert_eq!(
            expand_log_level("debug,h2=trace"),
            "debug,h2=trace"
        );
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
}
