use std::collections::HashMap;

use blobstore_core::{BlobType, PublicAccessLevel};
use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;

/// A stream of body chunks coming in from an HTTP request (Put Blob / Put Block bodies).
pub type ByteStream = Pin<Box<dyn Stream<Item = std::io::Result<Bytes>> + Send>>;

#[derive(Debug, Clone)]
pub struct ContainerProps {
    pub name: String,
    pub public_access: PublicAccessLevel,
    pub metadata: HashMap<String, String>,
    pub etag: String,
    pub last_modified: String,
}

#[derive(Debug, Clone)]
pub struct BlobProps {
    pub name: String,
    pub blob_type: BlobType,
    pub content_length: u64,
    pub content_type: Option<String>,
    pub content_md5: Option<String>,
    pub content_encoding: Option<String>,
    pub content_language: Option<String>,
    pub cache_control: Option<String>,
    pub content_disposition: Option<String>,
    pub metadata: HashMap<String, String>,
    pub etag: String,
    pub last_modified: String,
}

/// Patch for Set Blob Properties: `None` means "leave unchanged", per Azure semantics
/// each provided header replaces the corresponding property wholesale.
#[derive(Debug, Clone, Default)]
pub struct BlobPropertiesPatch {
    pub content_type: Option<String>,
    pub content_md5: Option<String>,
    pub content_encoding: Option<String>,
    pub content_language: Option<String>,
    pub cache_control: Option<String>,
    pub content_disposition: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ContainerListPage {
    pub containers: Vec<ContainerProps>,
    pub next_marker: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BlobListEntry {
    pub props: BlobProps,
}

#[derive(Debug, Clone, Default)]
pub struct BlobListPage {
    pub blobs: Vec<BlobListEntry>,
    /// Virtual "directory" prefixes produced when a `delimiter` is used.
    pub blob_prefixes: Vec<String>,
    pub next_marker: Option<String>,
}

/// One entry in a `<BlockList>` commit request body.
#[derive(Debug, Clone)]
pub enum BlockRefKind {
    Committed,
    Uncommitted,
    Latest,
}

#[derive(Debug, Clone)]
pub struct BlockRef {
    pub block_id: String,
    pub kind: BlockRefKind,
}

#[derive(Debug, Clone)]
pub struct CommittedBlock {
    pub block_id: String,
    pub size: u64,
}

#[derive(Debug, Clone, Default)]
pub struct BlockListResult {
    pub committed: Vec<CommittedBlock>,
    pub uncommitted: Vec<CommittedBlock>,
}

/// A byte range request, inclusive `[start, end]`, already resolved against the
/// resource's total size (so `end` is never past `total_len - 1`).
#[derive(Debug, Clone, Copy)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

pub struct BlobPayload {
    pub props: BlobProps,
    pub total_len: u64,
    pub range: Option<ByteRange>,
    pub reader: Pin<Box<dyn tokio::io::AsyncRead + Send>>,
}
