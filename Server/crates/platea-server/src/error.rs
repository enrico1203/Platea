//! Errori dell'API: ogni errore ha un codice stabile e un messaggio che dice cosa è successo.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("Accesso richiesto")]
    Unauthorized,
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    TooManyRequests(String),
    #[error("{code}: {message}")]
    Coded { status: StatusCode, code: &'static str, message: String },
    #[error("errore interno: {0}")]
    Internal(#[from] anyhow::Error),
}

#[derive(Serialize)]
pub struct ErrorBody {
    pub error: String,
    pub message: String,
}

impl ApiError {
    pub fn coded(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        ApiError::Coded { status, code, message: message.into() }
    }
    pub fn not_found(what: &str) -> Self {
        ApiError::NotFound(format!("{what} non trovato"))
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => ApiError::NotFound("Elemento non trovato".into()),
            other => ApiError::Internal(other.into()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, "BadRequest", m.clone()),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized", self.to_string()),
            ApiError::Forbidden(m) => (StatusCode::FORBIDDEN, "Forbidden", m.clone()),
            ApiError::NotFound(m) => (StatusCode::NOT_FOUND, "NotFound", m.clone()),
            ApiError::Conflict(m) => (StatusCode::CONFLICT, "Conflict", m.clone()),
            ApiError::TooManyRequests(m) => (StatusCode::TOO_MANY_REQUESTS, "TooManyRequests", m.clone()),
            ApiError::Coded { status, code, message } => (*status, *code, message.clone()),
            ApiError::Internal(e) => {
                tracing::error!(error = ?e, "errore interno");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal", "Errore interno del server".to_string())
            }
        };
        (status, Json(ErrorBody { error: code.to_string(), message })).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
