//! The HTTP API: routes onto handlers, handlers onto the store.

mod error;
mod mods;

use axum::Router;
use axum::routing::{get, post, put};

use crate::Store;

/// The modhub HTTP API over `store`.
///
/// | Method | Path | Does |
/// | --- | --- | --- |
/// | `POST` | `/mods` | registers `{"name": …}` → `201` |
/// | `PUT` | `/mods/{name}/versions/{version}` | publishes a release → `201` |
/// | `GET` | `/mods/{name}/versions` | lists versions, lowest precedence first → `200` |
///
/// Every error is JSON, `{"error": code, "message": text}`: clients branch on the stable
/// `error` code, people read the `message`.
pub fn router(store: Store) -> Router {
    Router::new()
        .route("/mods", post(mods::register))
        .route("/mods/{name}/versions", get(mods::versions))
        .route("/mods/{name}/versions/{version}", put(mods::publish))
        .with_state(store)
}
