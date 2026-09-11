//! Shared Key request signing/verification, implemented as pure functions with
//! no HTTP-framework dependency so it's directly unit-testable against
//! Microsoft's published worked examples for the Blob/Queue "Shared Key"
//! authorization scheme (`2015-02-21`+ canonicalization).

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::AuthError;

type HmacSha256 = Hmac<Sha256>;

/// The subset of a request Shared Key canonicalization needs. Header values
/// are the raw (not yet lower-cased) values; callers pass whatever was present
/// on the wire, using an empty string for headers that were absent.
#[derive(Debug, Clone, Default)]
pub struct SharedKeyRequest {
    pub verb: String,
    pub content_encoding: String,
    pub content_language: String,
    pub content_length: String,
    pub content_md5: String,
    pub content_type: String,
    pub date: String,
    pub if_modified_since: String,
    pub if_match: String,
    pub if_none_match: String,
    pub if_unmodified_since: String,
    pub range: String,
    /// All `x-ms-*` headers present on the request, as `(lowercase-name, value)`.
    pub x_ms_headers: Vec<(String, String)>,
    /// `/account/container/blob`-style path, already URL-decoded.
    pub canonical_path: String,
    /// Query string parameters, as `(name, value)` — name case as received.
    pub query_params: Vec<(String, String)>,
}

fn canonicalized_headers(x_ms_headers: &[(String, String)]) -> String {
    let mut headers: Vec<(String, String)> = x_ms_headers
        .iter()
        .map(|(k, v)| (k.to_lowercase(), v.trim().to_string()))
        .collect();
    headers.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = String::new();
    for (k, v) in headers {
        out.push_str(&k);
        out.push(':');
        out.push_str(&v);
        out.push('\n');
    }
    out
}

fn canonicalized_resource(
    account: &str,
    canonical_path: &str,
    query_params: &[(String, String)],
) -> String {
    let mut out = format!("/{account}{canonical_path}");

    // Group values by lower-cased parameter name, Azure joins multiple values
    // for the same parameter name with commas, sorted by parameter name.
    let mut grouped: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for (k, v) in query_params {
        grouped.entry(k.to_lowercase()).or_default().push(v.clone());
    }
    for (k, mut values) in grouped {
        values.sort();
        out.push('\n');
        out.push_str(&k);
        out.push(':');
        out.push_str(&values.join(","));
    }
    out
}

/// Builds the full string-to-sign per the Shared Key algorithm.
pub fn string_to_sign(account: &str, req: &SharedKeyRequest) -> String {
    // Per spec: Content-Length is an empty string when it is zero.
    let content_length = if req.content_length == "0" {
        String::new()
    } else {
        req.content_length.clone()
    };

    let parts = [
        req.verb.as_str(),
        req.content_encoding.as_str(),
        req.content_language.as_str(),
        content_length.as_str(),
        req.content_md5.as_str(),
        req.content_type.as_str(),
        req.date.as_str(),
        req.if_modified_since.as_str(),
        req.if_match.as_str(),
        req.if_none_match.as_str(),
        req.if_unmodified_since.as_str(),
        req.range.as_str(),
    ];
    let mut s = parts.join("\n");
    s.push('\n');
    s.push_str(&canonicalized_headers(&req.x_ms_headers));
    s.push_str(&canonicalized_resource(
        account,
        &req.canonical_path,
        &req.query_params,
    ));
    s
}

fn hmac_sign(key_base64: &str, message: &str) -> Result<String, AuthError> {
    let key_bytes = B64
        .decode(key_base64)
        .map_err(|_| AuthError::MalformedCredential)?;
    let mut mac =
        HmacSha256::new_from_slice(&key_bytes).map_err(|_| AuthError::MalformedCredential)?;
    mac.update(message.as_bytes());
    Ok(B64.encode(mac.finalize().into_bytes()))
}

/// Verifies `provided_signature` (the base64 value after `SharedKey {account}:`)
/// against `req`, trying `key1` then `key2` (key rotation support), matching
/// real Azure behavior of accepting either configured key.
pub fn verify(
    account: &str,
    req: &SharedKeyRequest,
    provided_signature: &str,
    key1_base64: &str,
    key2_base64: Option<&str>,
) -> Result<(), AuthError> {
    let sts = string_to_sign(account, req);
    if let Ok(sig) = hmac_sign(key1_base64, &sts) {
        if constant_time_eq(&sig, provided_signature) {
            return Ok(());
        }
    }
    if let Some(key2) = key2_base64 {
        if let Ok(sig) = hmac_sign(key2, &sts) {
            if constant_time_eq(&sig, provided_signature) {
                return Ok(());
            }
        }
    }
    Err(AuthError::SignatureMismatch)
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
