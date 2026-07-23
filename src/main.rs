use llm_proxy::{app, config, state, util::log_filter};
use salvo::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let loaded = config::load_config().await?;
    init_tracing(loaded.config.log_level.as_deref())?;

    let state = state::AppState::from_loaded(loaded)?;
    let bind = state.bind_addr().await;
    let router = app::router(state);
    tracing::info!(
        bind = %bind,
        "starting HTTP server with HTTP/1.1 and cleartext HTTP/2"
    );
    let acceptor = TcpListener::new(bind).bind().await;
    Server::new(acceptor).serve(router).await;
    Ok(())
}

fn init_tracing(log_level: Option<&str>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let filter = log_filter::resolve_env_filter(log_level)?;

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init()?;
    Ok(())
}
