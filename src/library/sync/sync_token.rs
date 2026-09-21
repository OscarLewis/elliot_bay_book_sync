use crate::error::AppError;
use axum::http::{HeaderMap, HeaderValue};
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashMap, fmt};
use tracing::error;

pub const SYNC_TOKEN_HEADER: &str = "x-kobo-synctoken";
pub const SYNC_VERSION: &str = "1-1-0";
pub const SYNC_LAST_MODIFIED_ADDED_VERSION: &str = "1-1-0";
pub const SYNC_MIN_VERSION: &str = "1-0-0";

/// Represents the data payload within a sync token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncTokenData {
    pub raw_kobo_store_token: String,
    pub books_last_created: DateTime<Utc>,
    pub books_last_modified: DateTime<Utc>,
    pub archive_last_modified: DateTime<Utc>,
    pub reading_state_last_modified: DateTime<Utc>,
    pub tags_last_modified: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncToken {
    pub version: String,
    pub data: SyncTokenData,
}

impl SyncToken {
    /// Parse SyncToken from HTTP headers
    pub fn from_headers(headers: &HeaderMap) -> Self {
        Self {
            version: SYNC_VERSION.to_string(),
            data: SyncTokenData::from_headers(headers),
        }
    }

    /// Merge from store response headers
    pub fn merge_from_store_response(&mut self, headers: &HeaderMap) {
        self.data.merge_from_store_response(headers);
    }

    /// Add token to response headers
    pub fn to_headers(&self, headers: &mut HeaderMap) {
        match HeaderValue::from_str(&self.build_sync_token()) {
            Ok(value) => {
                headers.insert(SYNC_TOKEN_HEADER, value);
            }
            Err(e) => {
                error!(?e, "Sync token is not a valid header value");
                headers.insert(SYNC_TOKEN_HEADER, HeaderValue::from_static(""));
            }
        }
    }

    /// Build the encoded sync token
    pub fn build_sync_token(&self) -> String {
        self.data.build_sync_token()
    }
}
impl SyncTokenData {
    pub fn new(
        raw_kobo_store_token: String,
        books_last_created: DateTime<Utc>,
        books_last_modified: DateTime<Utc>,
        archive_last_modified: DateTime<Utc>,
        reading_state_last_modified: DateTime<Utc>,
        tags_last_modified: DateTime<Utc>,
    ) -> Self {
        Self {
            raw_kobo_store_token,
            books_last_created,
            books_last_modified,
            archive_last_modified,
            reading_state_last_modified,
            tags_last_modified,
        }
    }

    /// Creates a SyncTokenData with default values
    pub fn default_with_token(raw_kobo_store_token: String) -> Self {
        let min_date = Utc.timestamp_opt(0, 0).unwrap();
        Self {
            raw_kobo_store_token,
            books_last_created: min_date,
            books_last_modified: min_date,
            archive_last_modified: min_date,
            reading_state_last_modified: min_date,
            tags_last_modified: min_date,
        }
    }

    /// Decode base64 and parse JSON
    fn decode_and_parse(encoded: &str) -> Result<Value, AppError> {
        // Add padding if needed
        let padding = (4 - (encoded.len() % 4)) % 4;
        let padded = format!("{}{}", encoded, "=".repeat(padding));

        let decoded = STANDARD.decode(&padded)?;
        let json = serde_json::from_slice(&decoded)?;
        Ok(json)
    }

    /// Extract datetime from JSON field
    fn get_datetime_from_json(
        data: &Value,
        field: &str,
    ) -> Result<DateTime<Utc>, Box<dyn std::error::Error>> {
        let timestamp = data[field].as_i64().ok_or("Invalid timestamp")?;
        Utc.timestamp_opt(timestamp, 0)
            .single()
            .ok_or("Out of range".into())
    }

    /// Set Kobo store header
    pub fn set_kobo_store_header(&self, headers: &mut HeaderMap) {
        match HeaderValue::from_str(&self.raw_kobo_store_token) {
            Ok(value) => {
                headers.insert(SYNC_TOKEN_HEADER, value);
            }
            Err(e) => error!(?e, "Kobo store token is not a valid header value"),
        }
    }

    /// Merge from store response headers
    pub fn merge_from_store_response(&mut self, headers: &HeaderMap) {
        self.raw_kobo_store_token = headers
            .get(SYNC_TOKEN_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
            .unwrap_or_default();
    }

    /// Build the encoded sync token
    pub fn build_sync_token(&self) -> String {
        let token = json!({
            "version": SYNC_VERSION,
            "data": {
                "raw_kobo_store_token": self.raw_kobo_store_token,
                "books_last_modified": self.books_last_modified.timestamp(),
                "books_last_created": self.books_last_created.timestamp(),
                "archive_last_modified": self.archive_last_modified.timestamp(),
                "reading_state_last_modified": self.reading_state_last_modified.timestamp(),
                "tags_last_modified": self.tags_last_modified.timestamp(),
            }
        });

        let json_string = serde_json::to_string(&token).unwrap_or_default();
        let encoded = STANDARD.encode(json_string.as_bytes());
        // Remove padding for compact representation
        encoded.trim_end_matches('=').to_string()
    }

    /// Parse SyncToken out from HTTP headers
    pub fn from_headers(headers: &HeaderMap) -> Self {
        let Some(sync_token_header) = headers
            .get(SYNC_TOKEN_HEADER)
            .and_then(|v| v.to_str().ok())
            .filter(|s| !s.is_empty())
        else {
            return Self::default();
        };

        // Check if it's a raw Kobo store token (contains a dot)
        if sync_token_header.contains('.') {
            return Self::default_with_token(sync_token_header.to_string());
        }

        // Try to decode and parse JSON
        let sync_token_json = match Self::decode_and_parse(sync_token_header) {
            Ok(json) => json,
            Err(_) => {
                error!("Sync token contents do not follow the expected json schema.");
                return Self::default();
            }
        };

        // Validate version
        let version = match sync_token_json.get("version").and_then(|v| v.as_str()) {
            Some(v) if v >= SYNC_MIN_VERSION => v,
            _ => {
                error!("Sync token version check failed.");
                return Self::default();
            }
        };

        let data_json = match sync_token_json.get("data") {
            Some(d) => d,
            None => {
                error!("Sync token missing data field.");
                return Self::default();
            }
        };

        // Extract store token from the data response
        let raw_kobo_store_token = data_json
            .get("raw_kobo_store_token")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Parse timestamps
        match (
            Self::get_datetime_from_json(data_json, "books_last_modified"),
            Self::get_datetime_from_json(data_json, "books_last_created"),
            Self::get_datetime_from_json(data_json, "archive_last_modified"),
            Self::get_datetime_from_json(data_json, "reading_state_last_modified"),
            Self::get_datetime_from_json(data_json, "tags_last_modified"),
        ) {
            (Ok(blm), Ok(blc), Ok(alm), Ok(rslm), Ok(tlm)) => Self {
                raw_kobo_store_token,
                books_last_modified: blm,
                books_last_created: blc,
                archive_last_modified: alm,
                reading_state_last_modified: rslm,
                tags_last_modified: tlm,
            },
            _ => {
                log::error!("SyncToken timestamps don't parse to a datetime.");
                Self::default_with_token(raw_kobo_store_token)
            }
        }
    }
}

impl Default for SyncTokenData {
    fn default() -> Self {
        let min_date = Utc.timestamp_opt(0, 0).unwrap();
        Self {
            raw_kobo_store_token: String::new(),
            books_last_created: min_date,
            books_last_modified: min_date,
            archive_last_modified: min_date,
            reading_state_last_modified: min_date,
            tags_last_modified: min_date,
        }
    }
}

impl fmt::Display for SyncTokenData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{},{},{},{},{},{}",
            self.books_last_created,
            self.books_last_modified,
            self.archive_last_modified,
            self.reading_state_last_modified,
            self.tags_last_modified,
            self.raw_kobo_store_token
        )
    }
}

// Completely overblown test suite (used because SyncToken is the backbone of the process)
#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue};
    use chrono::Utc;

    #[test]
    fn test_default_creation() {
        let token = SyncTokenData::default();
        assert_eq!(token.raw_kobo_store_token, "");
        assert_eq!(token.books_last_created.timestamp(), 0);
        assert_eq!(token.books_last_modified.timestamp(), 0);
    }

    #[test]
    fn test_default_with_token() {
        let token = SyncTokenData::default_with_token("my-token".to_string());
        assert_eq!(token.raw_kobo_store_token, "my-token");
        assert_eq!(token.books_last_created.timestamp(), 0);
    }

    #[test]
    fn test_new_with_custom_values() {
        let now = Utc::now();
        let token = SyncTokenData::new("token123".to_string(), now, now, now, now, now);
        assert_eq!(token.raw_kobo_store_token, "token123");
        assert_eq!(token.books_last_created, now);
    }

    #[test]
    fn test_build_sync_token_encoding() {
        let token = SyncTokenData::default_with_token("test-token".to_string());
        let encoded = token.build_sync_token();

        // Should be base64 without padding
        assert!(!encoded.contains('='));
        assert!(!encoded.is_empty());
    }

    #[test]
    fn test_build_and_decode_roundtrip() {
        let original = SyncTokenData::default_with_token("roundtrip-token".to_string());
        let encoded = original.build_sync_token();

        let mut headers = HeaderMap::new();
        headers.insert(SYNC_TOKEN_HEADER, HeaderValue::from_str(&encoded).unwrap());

        let decoded = SyncTokenData::from_headers(&headers);
        assert_eq!(decoded.raw_kobo_store_token, "roundtrip-token");
    }

    #[test]
    fn test_from_headers_empty() {
        let headers = HeaderMap::new();
        let token = SyncTokenData::from_headers(&headers);
        assert_eq!(token.raw_kobo_store_token, "");
    }

    #[test]
    fn test_from_headers_raw_kobo_token() {
        let mut headers = HeaderMap::new();
        let raw_token = "blob1.blob2".to_string();
        headers.insert(
            SYNC_TOKEN_HEADER,
            HeaderValue::from_str(&raw_token).unwrap(),
        );

        let token = SyncTokenData::from_headers(&headers);
        assert_eq!(token.raw_kobo_store_token, raw_token);
    }

    #[test]
    fn test_from_headers_invalid_base64() {
        let mut headers = HeaderMap::new();
        headers.insert(SYNC_TOKEN_HEADER, HeaderValue::from_static("!!!invalid!!!"));

        let token = SyncTokenData::from_headers(&headers);
        assert_eq!(token.raw_kobo_store_token, "");
    }

    #[test]
    fn test_from_headers_missing_data_field() {
        let mut headers = HeaderMap::new();
        let invalid_json = json!({"version": "1-1-0"});
        let encoded = STANDARD.encode(serde_json::to_string(&invalid_json).unwrap().as_bytes());
        headers.insert(
            SYNC_TOKEN_HEADER,
            HeaderValue::from_str(encoded.trim_end_matches('=')).unwrap(),
        );

        let token = SyncTokenData::from_headers(&headers);
        assert_eq!(token.raw_kobo_store_token, "");
    }

    #[test]
    fn test_from_headers_version_too_old() {
        let mut headers = HeaderMap::new();
        let old_version = json!({
            "version": "0-9-0",
            "data": {"raw_kobo_store_token": "token"}
        });
        let encoded = STANDARD.encode(serde_json::to_string(&old_version).unwrap().as_bytes());
        headers.insert(
            SYNC_TOKEN_HEADER,
            HeaderValue::from_str(encoded.trim_end_matches('=')).unwrap(),
        );

        let token = SyncTokenData::from_headers(&headers);
        assert_eq!(token.raw_kobo_store_token, "");
    }

    #[test]
    fn test_set_kobo_store_header() {
        let token = SyncTokenData::default_with_token("store-token".to_string());
        let mut headers = HeaderMap::new();

        token.set_kobo_store_header(&mut headers);
        assert_eq!(
            headers.get(SYNC_TOKEN_HEADER),
            Some(&HeaderValue::from_static("store-token"))
        );
    }

    #[test]
    fn test_merge_from_store_response() {
        let mut token = SyncTokenData::default();
        let mut headers = HeaderMap::new();
        headers.insert(SYNC_TOKEN_HEADER, HeaderValue::from_static("merged-token"));

        token.merge_from_store_response(&headers);
        assert_eq!(token.raw_kobo_store_token, "merged-token");
    }

    #[test]
    fn test_to_headers() {
        let mut headers = HeaderMap::new();
        let data = SyncTokenData::default_with_token("header-token".to_string());
        let token = SyncToken {
            version: SYNC_VERSION.to_string(),
            data,
        };

        token.to_headers(&mut headers);
        assert!(headers.contains_key(SYNC_TOKEN_HEADER));
        assert!(!headers.get(SYNC_TOKEN_HEADER).unwrap().is_empty());
    }

    #[test]
    fn test_display_format() {
        let token = SyncTokenData::default_with_token("display-token".to_string());
        let display = token.to_string();

        assert!(display.contains("display-token"));
        assert!(display.contains("1970-01-01")); // Default epoch time
    }

    #[test]
    fn test_timestamps_with_real_dates() {
        let date1 = Utc.timestamp_opt(1609459200, 0).unwrap(); // 2021-01-01
        let date2 = Utc.timestamp_opt(1640995200, 0).unwrap(); // 2022-01-01

        let token = SyncTokenData::new("token".to_string(), date1, date2, date1, date2, date1);

        assert_eq!(token.books_last_created.timestamp(), 1609459200);
        assert_eq!(token.books_last_modified.timestamp(), 1640995200);
    }

    #[test]
    fn test_full_roundtrip_with_timestamps() {
        let now = Utc.timestamp_opt(1609459200, 0).unwrap();
        let original = SyncTokenData::new("full-test".to_string(), now, now, now, now, now);

        let encoded = original.build_sync_token();
        let mut headers = HeaderMap::new();
        headers.insert(SYNC_TOKEN_HEADER, HeaderValue::from_str(&encoded).unwrap());

        let decoded = SyncTokenData::from_headers(&headers);
        assert_eq!(decoded.raw_kobo_store_token, "full-test");
        assert_eq!(decoded.books_last_created.timestamp(), 1609459200);
        assert_eq!(decoded.books_last_modified.timestamp(), 1609459200);
    }

    #[test]
    fn test_base64_padding_handling() {
        // Test that padding is correctly added and removed
        let token = SyncTokenData::default_with_token("pad-test".to_string());
        let encoded = token.build_sync_token();

        // Encoded should not have padding
        assert!(!encoded.ends_with('='));

        // Should still decode properly
        let mut headers = HeaderMap::new();
        headers.insert(SYNC_TOKEN_HEADER, HeaderValue::from_str(&encoded).unwrap());
        let decoded = SyncTokenData::from_headers(&headers);
        assert_eq!(decoded.raw_kobo_store_token, "pad-test");
    }
}
