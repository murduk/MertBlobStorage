//! axum router + handlers for the Azure Blob Storage-compatible HTTP surface.
//! Every handler downstream of [`auth::auth_middleware`] is auth-scheme
//! agnostic: it only ever reads the resolved `AuthContext` from request
//! extensions (attached by the middleware), never which of Shared
//! Key/SAS/anonymous/bearer produced it.

mod auth;
mod error_response;
mod handlers;
mod helpers;
mod query;
mod state;

pub use state::AppState;

use axum::routing::{get, put};
use axum::Router;
use tower_http::trace::TraceLayer;

pub fn build_router(state: AppState) -> Router {
    let api_routes = Router::new()
        .route("/:account", get(handlers::account::list_containers))
        .route(
            "/:account/:container",
            put(handlers::containers::container_put)
                .delete(handlers::containers::container_delete)
                .get(handlers::containers::container_get)
                .head(handlers::containers::container_get),
        )
        .route(
            "/:account/:container/*blob",
            put(handlers::blobs::blob_put)
                .get(handlers::blobs::blob_get)
                .head(handlers::blobs::blob_get)
                .delete(handlers::blobs::blob_delete),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ))
        .with_state(state);

    Router::new()
        .route("/healthz", get(handlers::health::healthz))
        .route("/readyz", get(handlers::health::readyz))
        .merge(api_routes)
        .layer(TraceLayer::new_for_http())
}
