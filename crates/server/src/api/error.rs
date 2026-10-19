//! How a failed request is answered: a status and a JSON body with a stable code.

use std::borrow::Cow;

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use modhub_core::{ModNameError, ModVersionError};
use serde::Serialize;

use crate::StoreError;

/// Everything a handler can fail with.
#[derive(Debug, thiserror::Error)]
pub(super) enum ApiError {
    #[error(transparent)]
    InvalidName(#[from] ModNameError),
    #[error(transparent)]
    InvalidVersion(#[from] ModVersionError),
    #[error(transparent)]
    InvalidBody(#[from] JsonRejection),
    #[error(transparent)]
    Store(#[from] StoreError),
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
    message: Cow<'static, str>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error) = match &self {
            Self::InvalidName(_) => (StatusCode::UNPROCESSABLE_ENTITY, "invalid_name"),
            Self::InvalidVersion(_) => (StatusCode::UNPROCESSABLE_ENTITY, "invalid_version"),
            // Keeps axum's distinction: 400 bad JSON, 415 wrong content type, 422 wrong shape.
            Self::InvalidBody(rejection) => (rejection.status(), "invalid_body"),
            Self::Store(StoreError::NameTaken { .. }) => (StatusCode::CONFLICT, "name_taken"),
            Self::Store(StoreError::UnknownMod { .. }) => (StatusCode::NOT_FOUND, "unknown_mod"),
            Self::Store(StoreError::VersionExists { .. }) => {
                (StatusCode::CONFLICT, "version_exists")
            }
            // Our fault, and the details (SQL, row contents) are not the client's business:
            // they go to the log, the client gets a generic answer.
            Self::Store(StoreError::CorruptVersion { .. } | StoreError::Database(_)) => {
                let error: &dyn std::error::Error = &self;
                tracing::error!(error, "request failed in the store");
                let body = ErrorBody {
                    error: "internal",
                    message: Cow::Borrowed("internal server error"),
                };
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response();
            }
        };
        let body = ErrorBody {
            error,
            message: Cow::Owned(self.to_string()),
        };
        (status, Json(body)).into_response()
    }
}
