use axum::http::StatusCode;

/// Liveness probe: the process is up and serving requests.
pub async fn healthz() -> StatusCode {
    StatusCode::OK
}

/// Readiness probe: kept intentionally simple (liveness is sufficient once the
/// server has started, since startup already fails fast if the DB/storage root
/// aren't usable). A deeper check could re-probe disk/DB on each call if
/// needed later.
pub async fn readyz() -> StatusCode {
    StatusCode::OK
}
