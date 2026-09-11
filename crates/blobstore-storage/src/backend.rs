use std::collections::HashMap;

use async_trait::async_trait;
use blobstore_core::{AzureError, PublicAccessLevel};

use crate::types::{
    BlobListPage, BlobPayload, BlobPropertiesPatch, BlobProps, BlockListResult, BlockRef,
    ByteStream, ContainerListPage, ContainerProps,
};

/// Everything the HTTP layer needs from a storage implementation. Kept
/// storage-technology-agnostic (no SQL/filesystem types leak through this trait)
/// so a future backend (e.g. S3-backed, or a clustered store) can be swapped in
/// without touching `blobstore-api` or `blobstore-auth`.
#[async_trait]
pub trait StorageBackend: Send + Sync + 'static {
    // --- Accounts ---

    /// Returns `(key1, key2)` base64-encoded account keys, if the account exists.
    async fn account_keys(&self, account: &str) -> Option<(String, Option<String>)>;

    // --- Containers ---

    async fn create_container(
        &self,
        account: &str,
        container: &str,
        public_access: PublicAccessLevel,
        metadata: HashMap<String, String>,
    ) -> Result<ContainerProps, AzureError>;

    async fn delete_container(&self, account: &str, container: &str) -> Result<(), AzureError>;

    async fn get_container_properties(
        &self,
        account: &str,
        container: &str,
    ) -> Result<ContainerProps, AzureError>;

    async fn set_container_metadata(
        &self,
        account: &str,
        container: &str,
        metadata: HashMap<String, String>,
    ) -> Result<ContainerProps, AzureError>;

    async fn set_container_acl(
        &self,
        account: &str,
        container: &str,
        public_access: PublicAccessLevel,
    ) -> Result<ContainerProps, AzureError>;

    async fn list_containers(
        &self,
        account: &str,
        prefix: Option<&str>,
        marker: Option<&str>,
        max_results: u32,
    ) -> Result<ContainerListPage, AzureError>;

    async fn list_blobs(
        &self,
        account: &str,
        container: &str,
        prefix: Option<&str>,
        delimiter: Option<&str>,
        marker: Option<&str>,
        max_results: u32,
    ) -> Result<BlobListPage, AzureError>;

    // --- Blobs ---

    /// Single-shot Put Blob: streams `body` directly to disk without buffering
    /// the whole payload in memory.
    #[allow(clippy::too_many_arguments)]
    async fn put_blob(
        &self,
        account: &str,
        container: &str,
        blob: &str,
        body: ByteStream,
        content_type: Option<String>,
        content_md5_header: Option<String>,
        metadata: HashMap<String, String>,
    ) -> Result<BlobProps, AzureError>;

    /// Returns blob properties plus a streaming reader over `range` (or the
    /// whole blob when `range` is `None`).
    async fn get_blob(
        &self,
        account: &str,
        container: &str,
        blob: &str,
        range: Option<(u64, u64)>,
    ) -> Result<BlobPayload, AzureError>;

    async fn get_blob_properties(
        &self,
        account: &str,
        container: &str,
        blob: &str,
    ) -> Result<BlobProps, AzureError>;

    async fn set_blob_properties(
        &self,
        account: &str,
        container: &str,
        blob: &str,
        patch: BlobPropertiesPatch,
    ) -> Result<BlobProps, AzureError>;

    async fn set_blob_metadata(
        &self,
        account: &str,
        container: &str,
        blob: &str,
        metadata: HashMap<String, String>,
    ) -> Result<BlobProps, AzureError>;

    async fn delete_blob(
        &self,
        account: &str,
        container: &str,
        blob: &str,
    ) -> Result<(), AzureError>;

    async fn put_block(
        &self,
        account: &str,
        container: &str,
        blob: &str,
        block_id: &str,
        body: ByteStream,
    ) -> Result<String, AzureError>;

    async fn put_block_list(
        &self,
        account: &str,
        container: &str,
        blob: &str,
        blocks: Vec<BlockRef>,
        content_type: Option<String>,
        metadata: HashMap<String, String>,
    ) -> Result<BlobProps, AzureError>;

    async fn get_block_list(
        &self,
        account: &str,
        container: &str,
        blob: &str,
    ) -> Result<BlockListResult, AzureError>;
}
