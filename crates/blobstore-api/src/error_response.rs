use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use blobstore_core::{AzureError, AzureErrorCode};

/// Local wrapper so handlers can `?`-propagate an [`AzureError`] straight into
/// an axum response (Rust's orphan rules block `impl IntoResponse for
/// AzureError` directly, since both the trait and the type are foreign here).
pub struct ApiError(pub AzureError);

impl From<AzureError> for ApiError {
    fn from(e: AzureError) -> Self {
        ApiError(e)
    }
}

impl From<AzureErrorCode> for ApiError {
    fn from(c: AzureErrorCode) -> Self {
        ApiError(AzureError::new(c))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        azure_error_response(&self.0, &uuid::Uuid::new_v4().to_string())
    }
}

/// Converts an [`AzureError`] into the standard Azure-shaped XML error
/// response: matching HTTP status, `x-ms-error-code` header, and `<Error>` body.
pub fn azure_error_response(err: &AzureError, request_id: &str) -> Response {
    let body = blobstore_xml::render_error(err, request_id);
    let status =
        StatusCode::from_u16(err.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut resp = (status, body).into_response();
    resp.headers_mut().insert(
        "x-ms-error-code",
        HeaderValue::from_str(err.code.code_str())
            .unwrap_or(HeaderValue::from_static("InternalError")),
    );
    resp.headers_mut().insert(
        "x-ms-request-id",
        HeaderValue::from_str(request_id).unwrap_or(HeaderValue::from_static("-")),
    );
    resp.headers_mut()
        .insert("Content-Type", HeaderValue::from_static("application/xml"));
    resp
}
