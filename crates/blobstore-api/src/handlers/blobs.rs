use axum::body::Body;
use axum::extract::{Path, RawQuery, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use blobstore_storage::BlobPropertiesPatch;
use tokio_util::io::ReaderStream;

use crate::error_response::ApiError;
use crate::helpers::{
    apply_blob_property_headers, apply_metadata_headers, body_to_stream, extract_metadata,
    parse_range,
};
use crate::query::{self, parse_query};
use crate::state::AppState;

type BlobPath = Path<(String, String, String)>;

fn header_opt(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// `PUT /{account}/{container}/{blob}[?comp=block|blocklist|properties|metadata]`
pub async fn blob_put(
    State(state): State<AppState>,
    Path((account, container, blob)): BlobPath,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, ApiError> {
    let params = parse_query(raw.as_deref().unwrap_or(""));
    let comp = query::get(&params, "comp");

    match comp {
        Some("block") => {
            let block_id = query::get(&params, "blockid")
                .unwrap_or_default()
                .to_string();
            let stream = body_to_stream(body);
            let md5 = state
                .backend
                .put_block(&account, &container, &blob, &block_id, stream)
                .await?;
            let mut resp_headers = HeaderMap::new();
            if !md5.is_empty() {
                resp_headers.insert(
                    axum::http::HeaderName::from_static("content-md5"),
                    HeaderValue::from_str(&md5).unwrap(),
                );
            }
            Ok((StatusCode::CREATED, resp_headers).into_response())
        }
        Some("blocklist") => {
            let body_bytes = axum::body::to_bytes(body, 64 * 1024 * 1024)
                .await
                .map_err(|_| ApiError::from(blobstore_core::AzureErrorCode::InvalidXmlDocument))?;
            let refs = blobstore_xml::parse_block_list_request(&body_bytes)?;
            let content_type = header_opt(&headers, "content-type");
            let metadata = extract_metadata(&headers);
            let props = state
                .backend
                .put_block_list(&account, &container, &blob, refs, content_type, metadata)
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
        Some("properties") => {
            let patch = BlobPropertiesPatch {
                content_type: header_opt(&headers, "x-ms-blob-content-type"),
                content_md5: header_opt(&headers, "x-ms-blob-content-md5"),
                content_encoding: header_opt(&headers, "x-ms-blob-content-encoding"),
                content_language: header_opt(&headers, "x-ms-blob-content-language"),
                cache_control: header_opt(&headers, "x-ms-blob-cache-control"),
                content_disposition: header_opt(&headers, "x-ms-blob-content-disposition"),
            };
            let props = state
                .backend
                .set_blob_properties(&account, &container, &blob, patch)
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
        Some("metadata") => {
            let metadata = extract_metadata(&headers);
            let props = state
                .backend
                .set_blob_metadata(&account, &container, &blob, metadata)
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
            let blob_type = header_opt(&headers, "x-ms-blob-type").unwrap_or_default();
            if !blob_type.is_empty() && blob_type != "BlockBlob" {
                return Err(blobstore_core::AzureErrorCode::FeatureNotSupported.into());
            }
            let content_type = header_opt(&headers, "content-type");
            let content_md5 = header_opt(&headers, "content-md5");
            let metadata = extract_metadata(&headers);
            let stream = body_to_stream(body);
            let props = state
                .backend
                .put_blob(
                    &account,
                    &container,
                    &blob,
                    stream,
                    content_type,
                    content_md5,
                    metadata,
                )
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
            if let Some(md5) = &props.content_md5 {
                resp_headers.insert(
                    axum::http::HeaderName::from_static("content-md5"),
                    HeaderValue::from_str(md5).unwrap(),
                );
            }
            Ok((StatusCode::CREATED, resp_headers).into_response())
        }
    }
}

/// `GET`/`HEAD /{account}/{container}/{blob}[?comp=metadata|blocklist]`
pub async fn blob_get(
    State(state): State<AppState>,
    Path((account, container, blob)): BlobPath,
    RawQuery(raw): RawQuery,
    method: Method,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let params = parse_query(raw.as_deref().unwrap_or(""));
    let comp = query::get(&params, "comp");

    match comp {
        Some("metadata") => {
            let props = state
                .backend
                .get_blob_properties(&account, &container, &blob)
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
            apply_metadata_headers(&mut resp_headers, &props.metadata);
            Ok((StatusCode::OK, resp_headers).into_response())
        }
        Some("blocklist") => {
            let result = state
                .backend
                .get_block_list(&account, &container, &blob)
                .await?;
            let body = blobstore_xml::render_block_list_response(&result);
            Ok(([("Content-Type", "application/xml")], body).into_response())
        }
        _ => {
            if method == Method::HEAD {
                let props = state
                    .backend
                    .get_blob_properties(&account, &container, &blob)
                    .await?;
                let mut resp_headers = HeaderMap::new();
                apply_blob_property_headers(&mut resp_headers, &props);
                return Ok((StatusCode::OK, resp_headers).into_response());
            }

            let range = headers
                .get(axum::http::header::RANGE)
                .and_then(|v| v.to_str().ok())
                .and_then(parse_range);

            let payload = state
                .backend
                .get_blob(&account, &container, &blob, range)
                .await?;
            let mut resp_headers = HeaderMap::new();
            apply_blob_property_headers(&mut resp_headers, &payload.props);

            let status = if let Some(r) = payload.range {
                resp_headers.insert(
                    axum::http::header::CONTENT_RANGE,
                    HeaderValue::from_str(&format!(
                        "bytes {}-{}/{}",
                        r.start, r.end, payload.total_len
                    ))
                    .unwrap(),
                );
                resp_headers.insert(
                    axum::http::header::CONTENT_LENGTH,
                    HeaderValue::from_str(&(r.end - r.start + 1).to_string()).unwrap(),
                );
                StatusCode::PARTIAL_CONTENT
            } else {
                StatusCode::OK
            };

            let stream = ReaderStream::new(payload.reader);
            let body = Body::from_stream(stream);
            Ok((status, resp_headers, body).into_response())
        }
    }
}

/// `DELETE /{account}/{container}/{blob}`
pub async fn blob_delete(
    State(state): State<AppState>,
    Path((account, container, blob)): BlobPath,
) -> Result<Response, ApiError> {
    state
        .backend
        .delete_blob(&account, &container, &blob)
        .await?;
    Ok(StatusCode::ACCEPTED.into_response())
}
