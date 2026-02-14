//! Unified API error type — every 4xx/5xx response is JSON with a consistent shape.
//!
//! ```json
//! { "code": "not_found", "message": "receipt not found", "request_id": "..." }
//! ```

use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u64>,
}

#[derive(Debug)]
pub struct AppError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub retry_after_secs: Option<u64>,
    /// Extra headers to include (e.g. Allow, Retry-After).
    pub extra_headers: Vec<(String, String)>,
}

impl AppError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "bad_request",
            message: msg.into(),
            retry_after_secs: None,
            extra_headers: vec![],
        }
    }

    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
            message: msg.into(),
            retry_after_secs: None,
            extra_headers: vec![],
        }
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "forbidden",
            message: msg.into(),
            retry_after_secs: None,
            extra_headers: vec![],
        }
    }

    pub fn not_found(resource: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: format!("{resource} not found"),
            retry_after_secs: None,
            extra_headers: vec![],
        }
    }

    pub fn method_not_allowed(allowed: &str) -> Self {
        Self {
            status: StatusCode::METHOD_NOT_ALLOWED,
            code: "method_not_allowed",
            message: "method not allowed".into(),
            retry_after_secs: None,
            extra_headers: vec![("allow".into(), allowed.into())],
        }
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: "conflict",
            message: msg.into(),
            retry_after_secs: None,
            extra_headers: vec![],
        }
    }

    pub fn unsupported_media_type() -> Self {
        Self {
            status: StatusCode::UNSUPPORTED_MEDIA_TYPE,
            code: "unsupported_media_type",
            message: "content-type must be application/json".into(),
            retry_after_secs: None,
            extra_headers: vec![],
        }
    }

    pub fn too_many_requests(msg: impl Into<String>, retry_after: u64) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            code: "rate_limited",
            message: msg.into(),
            retry_after_secs: Some(retry_after),
            extra_headers: vec![("retry-after".into(), retry_after.to_string())],
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            message: msg.into(),
            retry_after_secs: None,
            extra_headers: vec![],
        }
    }

    pub fn unprocessable(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "unprocessable_entity",
            message: msg.into(),
            retry_after_secs: None,
            extra_headers: vec![],
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let body = ApiErrorBody {
            code: self.code,
            message: self.message,
            request_id: None, // TODO: extract from x-request-id extension
            retry_after_secs: self.retry_after_secs,
        };
        let mut resp = (self.status, Json(body)).into_response();
        resp.headers_mut().insert(
            header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        for (k, v) in &self.extra_headers {
            if let (Ok(name), Ok(val)) = (
                k.parse::<axum::http::header::HeaderName>(),
                v.parse::<axum::http::header::HeaderValue>(),
            ) {
                resp.headers_mut().insert(name, val);
            }
        }
        resp
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}: {}", self.status.as_u16(), self.code, self.message)
    }
}

impl std::error::Error for AppError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_body_serializes_without_optional_fields() {
        let body = ApiErrorBody {
            code: "not_found",
            message: "receipt not found".into(),
            request_id: None,
            retry_after_secs: None,
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["code"], "not_found");
        assert_eq!(json["message"], "receipt not found");
        assert!(json.get("request_id").is_none());
        assert!(json.get("retry_after_secs").is_none());
    }

    #[test]
    fn rate_limit_error_includes_retry_after() {
        let body = ApiErrorBody {
            code: "rate_limited",
            message: "too many requests".into(),
            request_id: None,
            retry_after_secs: Some(5),
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["retry_after_secs"], 5);
    }
}
