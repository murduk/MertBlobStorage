//! Works out which account a request targets before routing, so that clients
//! which leave the account out of the URL path still reach `/:account/...`.
//!
//! Azure SDKs only send path-style URLs (`http://host/{account}/container`)
//! when the endpoint host is an IP address; for a hostname they assume the
//! account lives in the host (`{account}.blob.core.windows.net`) and send
//! `/container` instead. Resolution order:
//! 1. first path segment is a known account → path-style, left untouched;
//! 2. first label of the `Host` header is a known account → host-style;
//! 3. `server.default_account` is configured → that account.
//!
//! When the path is rewritten, the original path is kept in [`SignedPath`]:
//! it is what the client used to build its Shared Key signature.

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, Uri};
use axum::middleware::Next;
use axum::response::Response;

use crate::state::AppState;

/// The request path as received, before any account prefix was inserted.
#[derive(Clone, Debug)]
pub struct SignedPath(pub String);

/// Returns the account to prepend to `path`, or `None` to leave it unchanged.
/// `first_segment_known` / `host_label_known` say whether those candidates
/// name an existing account.
fn resolve<'a>(
    host_label: Option<&'a str>,
    first_segment_known: bool,
    host_label_known: bool,
    default_account: Option<&'a str>,
) -> Option<&'a str> {
    if first_segment_known {
        return None;
    }
    if host_label_known {
        return host_label;
    }
    default_account
}

fn first_segment(path: &str) -> Option<&str> {
    path.trim_start_matches('/')
        .split('/')
        .next()
        .filter(|s| !s.is_empty())
}

/// `chambo.mertblobstorage:8443` → `chambo`; bare hosts (no dot) and IP
/// addresses have no account label.
fn host_label(host: &str) -> Option<&str> {
    let host = host.rsplit_once(':').map_or(host, |(h, port)| {
        if port.chars().all(|c| c.is_ascii_digit()) {
            h
        } else {
            host
        }
    });
    if host.parse::<std::net::IpAddr>().is_ok() || host.starts_with('[') {
        return None;
    }
    host.split_once('.')
        .map(|(label, _)| label)
        .filter(|l| !l.is_empty())
}

pub async fn account_resolve_middleware(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();

    let first_segment_known = match first_segment(&path) {
        Some(seg) => state.backend.account_keys(seg).await.is_some(),
        None => false,
    };
    let host = req
        .headers()
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .or_else(|| req.uri().host())
        .map(str::to_string);
    let label = host.as_deref().and_then(host_label);
    let host_label_known = match label {
        Some(l) if !first_segment_known => state.backend.account_keys(l).await.is_some(),
        _ => false,
    };

    if let Some(account) = resolve(
        label,
        first_segment_known,
        host_label_known,
        state.default_account.as_deref(),
    ) {
        // Account-level requests (`/?comp=list`) map to `/{account}`, the
        // route List Containers is mounted on.
        let rest = if path == "/" { "" } else { path.as_str() };
        let new_path_and_query = match req.uri().query() {
            Some(q) => format!("/{account}{rest}?{q}"),
            None => format!("/{account}{rest}"),
        };
        let mut parts = req.uri().clone().into_parts();
        if let Ok(pq) = new_path_and_query.parse() {
            parts.path_and_query = Some(pq);
            if let Ok(uri) = Uri::from_parts(parts) {
                *req.uri_mut() = uri;
                req.extensions_mut().insert(SignedPath(path));
            }
        }
    }

    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_style_is_left_alone() {
        assert_eq!(resolve(Some("x"), true, true, Some("dflt")), None);
    }

    #[test]
    fn host_label_wins_over_default() {
        assert_eq!(
            resolve(Some("chambo"), false, true, Some("dflt")),
            Some("chambo")
        );
    }

    #[test]
    fn falls_back_to_default_account() {
        assert_eq!(resolve(Some("x"), false, false, Some("dflt")), Some("dflt"));
        assert_eq!(resolve(None, false, false, Some("dflt")), Some("dflt"));
    }

    #[test]
    fn unknown_without_default_is_left_alone() {
        assert_eq!(resolve(Some("x"), false, false, None), None);
    }

    #[test]
    fn host_label_parsing() {
        assert_eq!(host_label("chambo.mertblobstorage:8443"), Some("chambo"));
        assert_eq!(host_label("chambo.example.org"), Some("chambo"));
        assert_eq!(host_label("mertblobstorage:8443"), None);
        assert_eq!(host_label("localhost"), None);
        assert_eq!(host_label("127.0.0.1:8443"), None);
        assert_eq!(host_label("[::1]:8443"), None);
    }

    #[test]
    fn first_segment_parsing() {
        assert_eq!(first_segment("/chambo/catalog"), Some("chambo"));
        assert_eq!(first_segment("/catalog"), Some("catalog"));
        assert_eq!(first_segment("/"), None);
    }
}
