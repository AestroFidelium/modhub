//! The modhub server.
//!
//! Configuration comes from the environment (see `modhub_server::Config`), logs go to
//! stderr filtered by `RUST_LOG` (default `info,sqlx=warn`). SIGTERM or Ctrl-C stops it after
//! in-flight requests finish.

use std::io::IsTerminal;

use anyhow::Context;
use modhub_server::{Config, Store, serve};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            // sqlx logs migration notices at INFO ("relation already exists") on every start.
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();

    let config = Config::from_env(|var| std::env::var_os(var)).context("reading configuration")?;
    tracing::info!(?config, "starting");

    let pool = PgPoolOptions::new()
        .connect_with(config.database().clone())
        .await
        .context("connecting to PostgreSQL")?;
    let store = Store::new(pool);
    store.migrate().await.context("applying migrations")?;

    // Installed before serving: a server that cannot be stopped cleanly must not start.
    let mut terminate =
        signal(SignalKind::terminate()).context("installing the SIGTERM handler")?;
    let mut interrupt = signal(SignalKind::interrupt()).context("installing the SIGINT handler")?;
    // SIGTERM is what systemd and container runtimes send; SIGINT is Ctrl-C.
    let shutdown = async move {
        tokio::select! {
            _ = terminate.recv() => {}
            _ = interrupt.recv() => {}
        }
        tracing::info!("shutdown signal received, finishing in-flight requests");
    };

    let listener = TcpListener::bind(config.listen())
        .await
        .with_context(|| format!("binding {}", config.listen()))?;
    tracing::info!(address = %listener.local_addr()?, "listening");

    serve(listener, store, shutdown).await.context("serving")?;
    tracing::info!("stopped");
    Ok(())
}
