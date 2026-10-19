//! Handlers under `/mods`.

use axum::Json;
use axum::body::Bytes;
use axum::extract::rejection::{BytesRejection, JsonRejection};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::IntoResponse;
use modhub_core::{EntryPoint, ModModule, ModName, ModVersion};
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
    sha256: String,
    size: usize,
    entry_points: Vec<&'static str>,
}

#[derive(Serialize)]
pub(super) struct Versions {
    name: String,
    versions: Vec<String>,
}

const WASM: &str = "application/wasm";

fn entry_point_name(entry: EntryPoint) -> &'static str {
    match entry {
        EntryPoint::Handle => "handle",
        EntryPoint::Trigger => "trigger",
        EntryPoint::Tick => "tick",
    }
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
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<(StatusCode, Json<Published>), ApiError> {
    // The body first: an oversized or broken upload is refused whatever its path says.
    let body = body?;
    if headers.get(header::CONTENT_TYPE) != Some(&HeaderValue::from_static(WASM)) {
        return Err(ApiError::NotWasm);
    }
    let parsed_name: ModName = name.parse()?;
    let parsed_version: ModVersion = version.parse()?;
    let module = ModModule::check(&body)?;
    let sha256 = store
        .publish(&parsed_name, &parsed_version, &module)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(Published {
            name,
            version,
            sha256: sha256.to_string(),
            size: body.len(),
            entry_points: module
                .module()
                .entry_points()
                .map(entry_point_name)
                .collect(),
        }),
    ))
}

pub(super) async fn download(
    State(store): State<Store>,
    Path((name, version)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let parsed_name: ModName = name.parse()?;
    let parsed_version: ModVersion = version.parse()?;
    let module = store.download(&parsed_name, &parsed_version).await?;
    let etag = format!("\"{}\"", module.sha256());
    Ok((
        [
            (header::CONTENT_TYPE, WASM.to_owned()),
            (header::ETAG, etag),
            // A published version never changes, so caches may keep it for good.
            (
                header::CACHE_CONTROL,
                "public, max-age=31536000, immutable".to_owned(),
            ),
        ],
        module.into_bytes(),
    ))
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
