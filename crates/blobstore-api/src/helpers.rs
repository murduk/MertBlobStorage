use std::collections::HashMap;

use axum::body::Body;
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use blobstore_storage::{BlobProps, ByteStream};
use futures::TryStreamExt;

/// Adapts an axum request body into the [`ByteStream`] shape the storage
/// backend consumes, so bodies are streamed straight to disk rather than
/// buffered in memory.
pub fn body_to_stream(body: Body) -> ByteStream {
    Box::pin(
        body.into_data_stream()
            .map_err(|e| std::io::Error::other(e.to_string())),
    )
}

/// Extracts `x-ms-meta-*` headers into a plain metadata map, key = the part
/// after the prefix (lower-cased, matching how clients typically send them).
pub fn extract_metadata(headers: &HeaderMap) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for (name, value) in headers.iter() {
        let n = name.as_str().to_lowercase();
        if let Some(key) = n.strip_prefix("x-ms-meta-") {
            if let Ok(v) = value.to_str() {
                out.insert(key.to_string(), v.to_string());
            }
        }
    }
    out
}

pub fn apply_metadata_headers(headers: &mut HeaderMap, metadata: &HashMap<String, String>) {
    for (k, v) in metadata {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(format!("x-ms-meta-{k}").as_bytes()),
            HeaderValue::from_str(v),
        ) {
            headers.insert(name, value);
        }
    }
}

pub fn apply_blob_property_headers(headers: &mut HeaderMap, props: &BlobProps) {
    if let Ok(v) = HeaderValue::from_str(&props.etag) {
        headers.insert(axum::http::header::ETAG, v);
    }
    if let Ok(v) = HeaderValue::from_str(&props.last_modified) {
        headers.insert(axum::http::header::LAST_MODIFIED, v);
    }
    headers.insert(
        axum::http::header::CONTENT_LENGTH,
        HeaderValue::from_str(&props.content_length.to_string()).unwrap(),
    );
    if let Some(ct) = &props.content_type {
        if let Ok(v) = HeaderValue::from_str(ct) {
            headers.insert(axum::http::header::CONTENT_TYPE, v);
        }
    }
    if let Some(md5) = &props.content_md5 {
        if let Ok(v) = HeaderValue::from_str(md5) {
            headers.insert(HeaderName::from_static("content-md5"), v);
        }
    }
    if let Some(v) = &props.content_encoding {
        if let Ok(hv) = HeaderValue::from_str(v) {
            headers.insert(axum::http::header::CONTENT_ENCODING, hv);
        }
    }
    if let Some(v) = &props.content_language {
        if let Ok(hv) = HeaderValue::from_str(v) {
            headers.insert(axum::http::header::CONTENT_LANGUAGE, hv);
        }
    }
    if let Some(v) = &props.cache_control {
        if let Ok(hv) = HeaderValue::from_str(v) {
            headers.insert(axum::http::header::CACHE_CONTROL, hv);
        }
    }
    if let Some(v) = &props.content_disposition {
        if let Ok(hv) = HeaderValue::from_str(v) {
            headers.insert(axum::http::header::CONTENT_DISPOSITION, hv);
        }
    }
    headers.insert(
        HeaderName::from_static("x-ms-blob-type"),
        HeaderValue::from_static(props.blob_type.as_str()),
    );
    apply_metadata_headers(headers, &props.metadata);
}

/// Parses a single-range `Range: bytes=start-end` header into `(start, end)`,
/// where `end` may be `u64::MAX` to mean "to the end of the resource" (caller
/// clamps against the actual size). Multi-range requests are not supported.
pub fn parse_range(value: &str) -> Option<(u64, u64)> {
    let value = value.strip_prefix("bytes=")?;
    if value.contains(',') {
        return None; // multi-range unsupported
    }
    let (start, end) = value.split_once('-')?;
    if start.is_empty() {
        // suffix range "-N": last N bytes; caller resolves against total size,
        // signal with u64::MAX sentinel pair handled by caller as unsupported
        // shorthand for now (rare from the target clients).
        return None;
    }
    let start: u64 = start.parse().ok()?;
    let end: u64 = if end.is_empty() {
        u64::MAX
    } else {
        end.parse().ok()?
    };
    Some((start, end))
}
