//! Single auth/authorization middleware wiring together all four schemes.
//! Runs before every handler; handlers only ever consult the [`AuthContext`]
//! left in request extensions, never the scheme that produced it.

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use blobstore_auth::{
    anonymous, bearer, sas, shared_key, AuthContext, AuthError, AuthMethod, Permission,
};
use blobstore_core::{AzureError, AzureErrorCode};
use uuid::Uuid;

use crate::error_response::azure_error_response;
use crate::query::{self, parse_query};
use crate::state::AppState;

fn header_str(headers: &HeaderMap, name: &str) -> String {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}

/// Splits `/account/container/blob/with/slashes` into (account, container?, blob?).
fn split_path(path: &str) -> (Option<String>, Option<String>, Option<String>) {
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        return (None, None, None);
    }
    let mut parts = trimmed.splitn(3, '/');
    let account = parts.next().map(|s| s.to_string());
    let container = parts
        .next()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let blob = parts
        .next()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    (account, container, blob)
}

fn required_permission(method: &Method, comp: Option<&str>) -> Permission {
    if comp == Some("list") {
        return Permission::List;
    }
    match *method {
        Method::GET | Method::HEAD => Permission::Read,
        Method::DELETE => Permission::Delete,
        _ => Permission::Write,
    }
}

pub async fn auth_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let (account, container, blob) = split_path(req.uri().path());

    let Some(account) = account else {
        return azure_error_response(
            &AzureError::new(AzureErrorCode::RequestUrlFailedToParse),
            &request_id,
        );
    };

    let Some((key1, key2)) = state.backend.account_keys(&account).await else {
        return azure_error_response(
            &AzureError::new(AzureErrorCode::AuthenticationFailed),
            &request_id,
        );
    };

    let query_raw = req.uri().query().unwrap_or("");
    let query_params = parse_query(query_raw);
    let comp = query::get(&query_params, "comp").map(|s| s.to_string());
    let is_blob_scope = blob.is_some();
    let permission = required_permission(req.method(), comp.as_deref());

    let headers = req.headers().clone();
    let auth_header = header_str(&headers, "authorization");

    let ctx_result: Result<AuthContext, AzureError> = if let Some(rest) = auth_header
        .strip_prefix("SharedKey ")
        .or_else(|| auth_header.strip_prefix("SharedKeyLite "))
    {
        verify_shared_key(&account, &key1, key2.as_deref(), rest, &req, &query_params)
    } else if let Some(token) = auth_header.strip_prefix("Bearer ") {
        verify_bearer(&state, &account, token).await
    } else if let Some(sas_params) = sas::parse_sas_query(&query_params) {
        verify_sas(
            &account,
            &key1,
            key2.as_deref(),
            &sas_params,
            container.as_deref(),
            blob.as_deref(),
            is_blob_scope,
            permission,
        )
    } else {
        verify_anonymous(
            &state,
            &account,
            container.as_deref(),
            comp.as_deref(),
            is_blob_scope,
            req.method(),
        )
        .await
    };

    let resp = match ctx_result {
        Ok(ctx) => {
            let mut req = req;
            req.extensions_mut().insert(ctx);
            req.extensions_mut().insert(RequestId(request_id.clone()));
            next.run(req).await
        }
        Err(err) => azure_error_response(&err, &request_id),
    };
    finalize_response(resp, &request_id, &state.service_version)
}

fn finalize_response(mut resp: Response, request_id: &str, version: &str) -> Response {
    use axum::http::HeaderName;
    let h = resp.headers_mut();
    if !h.contains_key(axum::http::header::DATE) {
        if let Ok(v) = axum::http::HeaderValue::from_str(&blobstore_core::format_http_date(
            time::OffsetDateTime::now_utc(),
        )) {
            h.insert(axum::http::header::DATE, v);
        }
    }
    if !h.contains_key("x-ms-version") {
        if let Ok(v) = axum::http::HeaderValue::from_str(version) {
            h.insert(HeaderName::from_static("x-ms-version"), v);
        }
    }
    if !h.contains_key("x-ms-request-id") {
        if let Ok(v) = axum::http::HeaderValue::from_str(request_id) {
            h.insert(HeaderName::from_static("x-ms-request-id"), v);
        }
    }
    resp
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct RequestId(pub String);

fn verify_shared_key(
    account: &str,
    key1: &str,
    key2: Option<&str>,
    header_rest: &str,
    req: &Request<Body>,
    query_params: &[(String, String)],
) -> Result<AuthContext, AzureError> {
    let Some((hdr_account, signature)) = header_rest.split_once(':') else {
        return Err(AzureErrorCode::InvalidAuthenticationInfo.into());
    };
    if hdr_account != account {
        return Err(AzureErrorCode::AuthenticationFailed.into());
    }

    let headers = req.headers();
    let has_xms_date = headers.contains_key("x-ms-date");
    let date_field = if has_xms_date {
        String::new()
    } else {
        header_str(headers, "date")
    };

    let x_ms_headers: Vec<(String, String)> = headers
        .iter()
        .filter(|(k, _)| k.as_str().to_lowercase().starts_with("x-ms-"))
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();

    let sk_req = shared_key::SharedKeyRequest {
        verb: req.method().as_str().to_string(),
        content_encoding: header_str(headers, "content-encoding"),
        content_language: header_str(headers, "content-language"),
        content_length: header_str(headers, "content-length"),
        content_md5: header_str(headers, "content-md5"),
        content_type: header_str(headers, "content-type"),
        date: date_field,
        if_modified_since: header_str(headers, "if-modified-since"),
        if_match: header_str(headers, "if-match"),
        if_none_match: header_str(headers, "if-none-match"),
        if_unmodified_since: header_str(headers, "if-unmodified-since"),
        range: header_str(headers, "range"),
        x_ms_headers,
        // `shared_key::canonicalized_resource` prepends "/{account}" itself, so
        // this must be the path *after* the account segment (e.g. "/container/blob"),
        // not the full request path.
        canonical_path: {
            let full = req.uri().path();
            let after_account = full
                .strip_prefix('/')
                .and_then(|p| p.strip_prefix(account))
                .unwrap_or(full);
            // Account-level requests (e.g. List Containers) canonicalize to
            // just "/{account}" with no trailing slash.
            after_account.to_string()
        },
        query_params: query_params.to_vec(),
    };

    shared_key::verify(account, &sk_req, signature, key1, key2)
        .map(|_| AuthContext {
            account: account.to_string(),
            method: AuthMethod::SharedKey,
        })
        .map_err(map_auth_error)
}

async fn verify_bearer(
    state: &AppState,
    account: &str,
    token: &str,
) -> Result<AuthContext, AzureError> {
    let Some(cache) = &state.oidc else {
        return Err(AzureError::with_message(
            AzureErrorCode::AuthenticationFailed,
            "Bearer authentication is not configured on this server.",
        ));
    };
    let claims = bearer::validate_token(cache, token)
        .await
        .map_err(map_auth_error)?;
    Ok(AuthContext {
        account: account.to_string(),
        method: AuthMethod::Bearer {
            subject: claims.sub,
        },
    })
}

#[allow(clippy::too_many_arguments)]
fn verify_sas(
    account: &str,
    key1: &str,
    key2: Option<&str>,
    params: &sas::SasParams,
    container: Option<&str>,
    blob: Option<&str>,
    is_blob_scope: bool,
    permission: Permission,
) -> Result<AuthContext, AzureError> {
    let Some(container) = container else {
        return Err(AzureErrorCode::AuthorizationFailure.into());
    };

    sas::verify_signature(account, container, blob, params, key1, key2).map_err(map_auth_error)?;
    sas::check_validity_window(params).map_err(map_auth_error)?;

    if !sas::resource_scope_matches(params, is_blob_scope) {
        return Err(AzureErrorCode::AuthorizationResourceTypeMismatch.into());
    }
    if !sas::has_permission(params, permission.sas_char()) {
        return Err(AzureErrorCode::AuthorizationPermissionMismatch.into());
    }

    Ok(AuthContext {
        account: account.to_string(),
        method: AuthMethod::Sas,
    })
}

async fn verify_anonymous(
    state: &AppState,
    account: &str,
    container: Option<&str>,
    comp: Option<&str>,
    is_blob_scope: bool,
    method: &Method,
) -> Result<AuthContext, AzureError> {
    let Some(container) = container else {
        // Account-level operations (e.g. List Containers) always require a
        // real credential; there's no anonymous equivalent in Azure either.
        return Err(AzureErrorCode::AuthenticationFailed.into());
    };

    let props = state
        .backend
        .get_container_properties(account, container)
        .await
        // Anonymous access to a nonexistent/private container reports 404
        // rather than 403, matching real Azure's behavior of not revealing
        // container existence to unauthenticated callers.
        .map_err(|_| AzureError::new(AzureErrorCode::ContainerNotFound))?;

    let allowed = if !matches!(*method, Method::GET | Method::HEAD) {
        false
    } else if is_blob_scope {
        anonymous::allows_blob_read(props.public_access)
    } else if comp == Some("list") {
        anonymous::allows_container_read(props.public_access)
    } else {
        // Get Container Properties without comp=list is allowed at the same
        // level as container listing.
        anonymous::allows_container_read(props.public_access)
    };

    if allowed {
        Ok(AuthContext {
            account: account.to_string(),
            method: AuthMethod::Anonymous,
        })
    } else {
        Err(AzureError::new(AzureErrorCode::ContainerNotFound))
    }
}

fn map_auth_error(e: AuthError) -> AzureError {
    match e {
        AuthError::NoCredentials
        | AuthError::MalformedCredential
        | AuthError::SignatureMismatch => AzureError::new(AzureErrorCode::AuthenticationFailed),
        AuthError::UnknownSigningKey | AuthError::JwksUnavailable => {
            AzureError::new(AzureErrorCode::AuthenticationFailed)
        }
        AuthError::Expired | AuthError::NotYetValid => {
            AzureError::new(AzureErrorCode::AuthenticationFailed)
        }
        AuthError::PermissionDenied => {
            AzureError::new(AzureErrorCode::AuthorizationPermissionMismatch)
        }
        AuthError::ResourceScopeMismatch => {
            AzureError::new(AzureErrorCode::AuthorizationResourceTypeMismatch)
        }
        AuthError::ClockSkewTooLarge => AzureError::new(AzureErrorCode::AuthenticationFailed),
        AuthError::UnknownAccount => AzureError::new(AzureErrorCode::AuthenticationFailed),
    }
}

#[allow(dead_code)]
fn unauthorized() -> StatusCode {
    StatusCode::UNAUTHORIZED
}
