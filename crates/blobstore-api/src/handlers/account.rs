use axum::extract::{Path, RawQuery, State};
use axum::response::{IntoResponse, Response};

use crate::error_response::ApiError;
use crate::query::{self, parse_query};
use crate::state::AppState;

/// `GET /{account}?comp=list` — List Containers.
pub async fn list_containers(
    State(state): State<AppState>,
    Path(account): Path<String>,
    RawQuery(raw): RawQuery,
) -> Result<Response, ApiError> {
    let params = parse_query(raw.as_deref().unwrap_or(""));
    let prefix = query::get(&params, "prefix");
    let marker = query::get(&params, "marker");
    let max_results: u32 = query::get(&params, "maxresults")
        .and_then(|s| s.parse().ok())
        .unwrap_or(5000);

    let page = state
        .backend
        .list_containers(&account, prefix, marker, max_results)
        .await?;

    let body = blobstore_xml::render_list_containers(
        &state.service_endpoint,
        prefix,
        marker,
        max_results,
        &page,
    );

    Ok(([("Content-Type", "application/xml")], body).into_response())
}
