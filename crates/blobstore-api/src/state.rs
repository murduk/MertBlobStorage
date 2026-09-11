use std::sync::Arc;

use blobstore_auth::bearer::JwksCache;
use blobstore_storage::StorageBackend;

#[derive(Clone)]
pub struct AppState {
    pub backend: Arc<dyn StorageBackend>,
    pub oidc: Option<Arc<JwksCache>>,
    /// `x-ms-version` value this server reports/accepts.
    pub service_version: String,
    /// Base URL used to fill `ServiceEndpoint` attributes in list responses.
    pub service_endpoint: String,
}
