//! Storage layer: the technology-agnostic [`StorageBackend`] trait plus a
//! filesystem + SQLite implementation ([`FsBackend`]). See `docs/design.md`.

mod backend;
mod fs_backend;
mod fs_io;
mod metadata_db;
mod path_layout;
#[cfg(test)]
mod tests;
mod types;

pub use backend::StorageBackend;
pub use fs_backend::FsBackend;
pub use metadata_db::MetadataDb;
pub use types::{
    BlobListEntry, BlobListPage, BlobPayload, BlobPropertiesPatch, BlobProps, BlockListResult,
    BlockRef, BlockRefKind, ByteRange, ByteStream, CommittedBlock, ContainerListPage,
    ContainerProps,
};
