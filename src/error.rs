use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use reqwest::header::InvalidHeaderValue;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
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

    #[error("Not Found error: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Method not allowed")]
    MethodNotAllowed,

    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),

    #[error("URL parse error: {0}")]
    Url(#[from] url::ParseError),

    #[error("Config error: {0}")]
    Config(#[from] config::ConfigError),

    #[error("base64 decode parse error: {0}")]
    Base64Decode(#[from] base64::DecodeError),

    #[error("Invalid header value: {0}")]
    InvalidHeaderValue(#[from] InvalidHeaderValue),

    #[error("HTTP error: {0}")]
    Http(#[from] axum::http::Error),

    #[error("MongoDB error: {0}")]
    MongoDB(#[from] mongodb::error::Error),

    #[error("BSON serialization error: {0}")]
    BsonSer(#[from] mongodb::bson::ser::Error),

    #[error("MongoDB did not return an ObjectId for the inserted document")]
    InvalidObjectId,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self {
            AppError::BsonSer(_)
            | AppError::Config(_)
            | AppError::MongoDB(_)
            | AppError::InvalidObjectId
            | AppError::Reqwest(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Json(_) => StatusCode::BAD_REQUEST,
            AppError::Uuid(_) => StatusCode::BAD_REQUEST,
            AppError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::InvalidPath(_) => StatusCode::BAD_REQUEST,
            AppError::Scan(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::DocError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Image(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Url(_) => StatusCode::BAD_REQUEST,
            AppError::Base64Decode(_) => StatusCode::BAD_REQUEST,
            AppError::Http(_) => StatusCode::BAD_REQUEST,
            AppError::InvalidHeaderValue(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        (status, self.to_string()).into_response()
    }
}
