//! The HTTP API: routes onto handlers, handlers onto the store.

mod error;
mod mods;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post, put};
use modhub_core::ModModule;

use crate::Store;

/// The modhub HTTP API over `store`.
///
/// | Method | Path | Does |
/// | --- | --- | --- |
/// | `POST` | `/mods` | registers `{"name": …}` → `201` |
/// | `PUT` | `/mods/{name}/versions/{version}` | publishes the `application/wasm` body → `201` |
/// | `GET` | `/mods/{name}/versions/{version}` | downloads the module → `200` |
/// | `GET` | `/mods/{name}/versions` | lists versions, lowest precedence first → `200` |
///
/// Every error is JSON, `{"error": code, "message": text}`: clients branch on the stable
/// `error` code, people read the `message`.
pub fn router(store: Store) -> Router {
    Router::new()
        .route("/mods", post(mods::register))
        .route("/mods/{name}/versions", get(mods::versions))
        .route(
            "/mods/{name}/versions/{version}",
            put(mods::publish).get(mods::download),
        )
        // Uploads past the module limit are refused while reading, not after buffering.
        .layer(DefaultBodyLimit::max(ModModule::MAX_BYTES))
        .with_state(store)
}
