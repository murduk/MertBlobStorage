use axum::extract::{Path, RawQuery, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use blobstore_core::PublicAccessLevel;

use crate::error_response::ApiError;
use crate::helpers::{apply_metadata_headers, extract_metadata};
use crate::query::{self, parse_query};
use crate::state::AppState;

/// `PUT /{account}/{container}?restype=container[&comp=metadata|acl]`
pub async fn container_put(
    State(state): State<AppState>,
    Path((account, container)): Path<(String, String)>,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let params = parse_query(raw.as_deref().unwrap_or(""));
    let comp = query::get(&params, "comp");

    match comp {
        Some("metadata") => {
            let metadata = extract_metadata(&headers);
            let props = state
                .backend
                .set_container_metadata(&account, &container, metadata)
                .await?;
            let mut resp_headers = HeaderMap::new();
            resp_headers.insert(
                axum::http::header::ETAG,
                HeaderValue::from_str(&props.etag).unwrap(),
            );
            resp_headers.insert(
                axum::http::header::LAST_MODIFIED,
                HeaderValue::from_str(&props.last_modified).unwrap(),
            );
            Ok((StatusCode::OK, resp_headers).into_response())
        }
        Some("acl") => {
            let public_access = PublicAccessLevel::from_header_value(
                headers
                    .get("x-ms-blob-public-access")
                    .and_then(|v| v.to_str().ok()),
            );
            let props = state
                .backend
                .set_container_acl(&account, &container, public_access)
                .await?;
            let mut resp_headers = HeaderMap::new();
            resp_headers.insert(
                axum::http::header::ETAG,
                HeaderValue::from_str(&props.etag).unwrap(),
            );
            resp_headers.insert(
                axum::http::header::LAST_MODIFIED,
                HeaderValue::from_str(&props.last_modified).unwrap(),
            );
            Ok((StatusCode::OK, resp_headers).into_response())
        }
        _ => {
            let public_access = PublicAccessLevel::from_header_value(
                headers
                    .get("x-ms-blob-public-access")
                    .and_then(|v| v.to_str().ok()),
            );
            let metadata = extract_metadata(&headers);
            let props = state
                .backend
                .create_container(&account, &container, public_access, metadata)
                .await?;
            let mut resp_headers = HeaderMap::new();
            resp_headers.insert(
                axum::http::header::ETAG,
                HeaderValue::from_str(&props.etag).unwrap(),
            );
            resp_headers.insert(
                axum::http::header::LAST_MODIFIED,
                HeaderValue::from_str(&props.last_modified).unwrap(),
            );
            Ok((StatusCode::CREATED, resp_headers).into_response())
        }
    }
}

/// `DELETE /{account}/{container}?restype=container`
pub async fn container_delete(
    State(state): State<AppState>,
    Path((account, container)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    state.backend.delete_container(&account, &container).await?;
    Ok(StatusCode::ACCEPTED.into_response())
}

/// `GET`/`HEAD /{account}/{container}?restype=container[&comp=metadata|acl|list]`
pub async fn container_get(
    State(state): State<AppState>,
    Path((account, container)): Path<(String, String)>,
    RawQuery(raw): RawQuery,
    method: axum::http::Method,
) -> Result<Response, ApiError> {
    let params = parse_query(raw.as_deref().unwrap_or(""));
    let comp = query::get(&params, "comp");

    match comp {
        Some("list") => {
            let prefix = query::get(&params, "prefix");
            let delimiter = query::get(&params, "delimiter");
            let marker = query::get(&params, "marker");
            let max_results: u32 = query::get(&params, "maxresults")
                .and_then(|s| s.parse().ok())
                .unwrap_or(5000);
            let page = state
                .backend
                .list_blobs(&account, &container, prefix, delimiter, marker, max_results)
                .await?;
            let body = blobstore_xml::render_list_blobs(
                &state.service_endpoint,
                &container,
                prefix,
                delimiter,
                marker,
                max_results,
                &page,
            );
            Ok(([("Content-Type", "application/xml")], body).into_response())
        }
        Some("acl") => {
            let props = state
                .backend
                .get_container_properties(&account, &container)
                .await?;
            let body =
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<SignedIdentifiers/>".to_string();
            let mut resp_headers = HeaderMap::new();
            if let Some(pa) = props.public_access.as_header_value() {
                resp_headers.insert(
                    axum::http::HeaderName::from_static("x-ms-blob-public-access"),
                    HeaderValue::from_static(pa),
                );
            }
            resp_headers.insert(
                axum::http::header::ETAG,
                HeaderValue::from_str(&props.etag).unwrap(),
            );
            resp_headers.insert(
                axum::http::header::LAST_MODIFIED,
                HeaderValue::from_str(&props.last_modified).unwrap(),
            );
            resp_headers.insert(
                axum::http::header::CONTENT_TYPE,
                HeaderValue::from_static("application/xml"),
            );
            let body = if method == axum::http::Method::HEAD {
                String::new()
            } else {
                body
            };
            Ok((resp_headers, body).into_response())
        }
        _ => {
            // Get Container Properties / Get Container Metadata (same payload
            // shape; metadata is always included in properties).
            let props = state
                .backend
                .get_container_properties(&account, &container)
                .await?;
            let mut resp_headers = HeaderMap::new();
            resp_headers.insert(
                axum::http::header::ETAG,
                HeaderValue::from_str(&props.etag).unwrap(),
            );
            resp_headers.insert(
                axum::http::header::LAST_MODIFIED,
                HeaderValue::from_str(&props.last_modified).unwrap(),
            );
            if let Some(pa) = props.public_access.as_header_value() {
                resp_headers.insert(
                    axum::http::HeaderName::from_static("x-ms-blob-public-access"),
                    HeaderValue::from_static(pa),
                );
            }
            apply_metadata_headers(&mut resp_headers, &props.metadata);
            Ok((StatusCode::OK, resp_headers).into_response())
        }
    }
}
