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
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let state = state::AppState::load()?;
    let bind = state.bind_addr().await;
    let router = app::router(state);

    tracing::info!(%bind, "starting llm-proxy");
    let acceptor = TcpListener::new(bind).bind().await;
    Server::new(acceptor).serve(router).await;
    Ok(())
}
