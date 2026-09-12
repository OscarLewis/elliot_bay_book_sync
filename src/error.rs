use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] redb::DatabaseError),

    #[error("Redb error: {0}")]
    Redb(#[from] redb::Error),

    #[error("Database table error: {0}")]
    Table(#[from] redb::TableError),

    #[error("Transaction error: {0}")]
    Transaction(#[from] redb::TransactionError),

    #[error("Commit error: {0}")]
    Commit(#[from] redb::CommitError),

    #[error("Storage initialization error: {0}")]
    Storage(#[from] redb::StorageError),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("UUID parse error: {0}")]
    Uuid(#[from] uuid::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP client error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Scan error: {0}")]
    Scan(String),

    #[error("Epub error: {0}")]
    DocError(#[from] epub::doc::DocError),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self {
            AppError::Database(_)
            | AppError::Redb(_)
            | AppError::Table(_)
            | AppError::Transaction(_)
            | AppError::Commit(_)
            | AppError::Storage(_)
            | AppError::Reqwest(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Json(_) => StatusCode::BAD_REQUEST,
            AppError::Uuid(_) => StatusCode::BAD_REQUEST,
            AppError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::InvalidPath(_) => StatusCode::BAD_REQUEST,
            AppError::Scan(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::DocError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Image(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        (status, self.to_string()).into_response()
    }
}
