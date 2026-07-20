mod app;
mod config;
mod error;
mod middleware;
mod protocol;
mod provider;
mod retry;
mod state;
mod stream;

use salvo::prelude::*;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let loaded = config::load_config()?;
    init_tracing(loaded.config.log_level.as_deref())?;

    let state = state::AppState::from_loaded(loaded)?;
    let bind = state.bind_addr().await;
    let router = app::router(state);

    tracing::info!(%bind, "starting llm-proxy");
    let acceptor = TcpListener::new(bind).bind().await;
    Server::new(acceptor).serve(router).await;
    Ok(())
}

fn init_tracing(log_level: Option<&str>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) => {
            let directive = log_level
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("info");
            EnvFilter::try_new(directive)?
        }
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init()?;
    Ok(())
}
