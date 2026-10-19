//! Running the API on a socket until told to stop.

use std::future::Future;

use tokio::net::TcpListener;

use crate::{Store, router};

/// Serves the API on `listener` until `shutdown` completes, then stops accepting and
/// lets in-flight requests finish before returning.
///
/// The caller binds the listener, so it can log the actual address (port 0 picks a
/// free one) and tests can run many servers side by side.
///
/// # Errors
///
/// When the listener fails with an IO error that is not per-connection.
pub async fn serve(
    listener: TcpListener,
    store: Store,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(listener, router(store))
        .with_graceful_shutdown(shutdown)
        .await
}
