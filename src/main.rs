use llm_proxy::{
    app, config, state,
    util::log_filter,
    util::{DumpHub, LogHub},
};
use salvo::prelude::*;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::{fmt, prelude::*, reload};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let loaded = config::load_config().await?;
    let log_hub = LogHub::new();
    let dump_hub = DumpHub::new();
    init_tracing(loaded.config.log_level.as_deref(), log_hub.clone())?;

    let state = state::AppState::from_loaded(loaded, log_hub, dump_hub)?;
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

fn init_tracing(
    log_level: Option<&str>,
    log_hub: LogHub,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let filter = log_filter::resolve_env_filter(log_level)?;
    let (filter_layer, reload_handle) = reload::Layer::new(filter);
    log_filter::install_reload_handle(reload_handle);

    let writer = std::io::stdout.and(log_hub);
    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt::layer().with_writer(writer))
        .try_init()?;
    Ok(())
}
