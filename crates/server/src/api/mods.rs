//! Handlers under `/mods`.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use modhub_core::{ModName, ModVersion};
use serde::{Deserialize, Serialize};

use super::error::ApiError;
use crate::Store;

#[derive(Deserialize)]
pub(super) struct Registration {
    name: String,
}

#[derive(Serialize)]
pub(super) struct Registered {
    name: String,
}

#[derive(Serialize)]
pub(super) struct Published {
    name: String,
    version: String,
}

#[derive(Serialize)]
pub(super) struct Versions {
    name: String,
    versions: Vec<String>,
}

// Responses echo the request's text: a parsed name or version displays exactly as the
// text it came from (both parsers accept one spelling per value), so no re-rendering.

pub(super) async fn register(
    State(store): State<Store>,
    body: Result<Json<Registration>, JsonRejection>,
) -> Result<(StatusCode, Json<Registered>), ApiError> {
    let Json(Registration { name }) = body?;
    let parsed: ModName = name.parse()?;
    store.register(&parsed).await?;
    Ok((StatusCode::CREATED, Json(Registered { name })))
}

pub(super) async fn publish(
    State(store): State<Store>,
    Path((name, version)): Path<(String, String)>,
) -> Result<(StatusCode, Json<Published>), ApiError> {
    let parsed_name: ModName = name.parse()?;
    let parsed_version: ModVersion = version.parse()?;
    store.publish(&parsed_name, &parsed_version).await?;
    Ok((StatusCode::CREATED, Json(Published { name, version })))
}

pub(super) async fn versions(
    State(store): State<Store>,
    Path(name): Path<String>,
) -> Result<Json<Versions>, ApiError> {
    let parsed: ModName = name.parse()?;
    let versions = store.versions(&parsed).await?;
    Ok(Json(Versions {
        name,
        versions: versions.iter().map(ToString::to_string).collect(),
    }))
}
