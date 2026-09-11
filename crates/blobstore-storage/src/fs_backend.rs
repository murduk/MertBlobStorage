use std::collections::HashMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use blobstore_core::{AzureError, AzureErrorCode, BlobType, PublicAccessLevel};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::backend::StorageBackend;
use crate::fs_io;
use crate::metadata_db::{BlobRow, ContainerRow, MetadataDb};
use crate::path_layout;
use crate::types::{
    BlobListEntry, BlobListPage, BlobPayload, BlobPropertiesPatch, BlobProps, BlockListResult,
    BlockRef, BlockRefKind, ByteRange, ByteStream, CommittedBlock, ContainerListPage,
    ContainerProps,
};

/// Filesystem + SQLite implementation of [`StorageBackend`]: blob bytes on disk,
/// metadata in an embedded SQLite database, with atomic write-then-rename for
/// durability. See `docs/design.md` for the on-disk layout.
pub struct FsBackend {
    db: MetadataDb,
    storage_root: PathBuf,
}

impl FsBackend {
    pub async fn open(storage_root: impl Into<PathBuf>, db_path: &Path) -> anyhow::Result<Self> {
        let storage_root = storage_root.into();
        tokio::fs::create_dir_all(&storage_root).await?;
        let db = MetadataDb::connect(db_path).await?;
        Ok(Self { db, storage_root })
    }

    pub async fn ensure_account(&self, account: &str, key1: &str, key2: Option<&str>) {
        self.db.ensure_account(account, key1, key2).await;
    }

    fn blob_paths(&self, account: &str, container: &str, blob: &str) -> (PathBuf, String) {
        let rel = PathBuf::from("accounts")
            .join(account)
            .join("containers")
            .join(container)
            .join(path_layout::blob_rel_path(blob));
        let abs = self.storage_root.join(&rel);
        (abs, rel.to_string_lossy().replace('\\', "/"))
    }

    fn staged_block_path(
        &self,
        account: &str,
        container: &str,
        blob: &str,
        block_id: &str,
    ) -> (PathBuf, String) {
        let rel = PathBuf::from("accounts")
            .join(account)
            .join("containers")
            .join(container)
            .join(path_layout::staged_block_rel_path(blob, block_id));
        let abs = self.storage_root.join(&rel);
        (abs, rel.to_string_lossy().replace('\\', "/"))
    }

    fn tmp_dir(&self, account: &str) -> PathBuf {
        self.storage_root.join("accounts").join(account).join("tmp")
    }
}

fn container_row_to_props(row: ContainerRow) -> ContainerProps {
    ContainerProps {
        name: row.name,
        public_access: row.public_access_level,
        metadata: row.metadata,
        etag: row.etag,
        last_modified: row.last_modified,
    }
}

fn blob_row_to_props(row: BlobRow) -> BlobProps {
    BlobProps {
        name: row.name,
        blob_type: BlobType::parse(&row.blob_type).unwrap_or(BlobType::BlockBlob),
        content_length: row.content_length,
        content_type: row.content_type,
        content_md5: row.content_md5,
        content_encoding: row.content_encoding,
        content_language: row.content_language,
        cache_control: row.cache_control,
        content_disposition: row.content_disposition,
        metadata: row.metadata,
        etag: row.etag,
        last_modified: row.last_modified,
    }
}

fn md5_hex_to_base64(hex_str: &str) -> Option<String> {
    let bytes = hex::decode(hex_str).ok()?;
    Some(B64.encode(bytes))
}

#[async_trait]
impl StorageBackend for FsBackend {
    async fn account_keys(&self, account: &str) -> Option<(String, Option<String>)> {
        self.db.account_keys(account).await
    }

    async fn create_container(
        &self,
        account: &str,
        container: &str,
        public_access: PublicAccessLevel,
        metadata: HashMap<String, String>,
    ) -> Result<ContainerProps, AzureError> {
        let row = self
            .db
            .create_container(account, container, public_access, metadata)
            .await?;
        let dir = path_layout::container_root(&self.storage_root, account, container);
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;
        Ok(container_row_to_props(row))
    }

    async fn delete_container(&self, account: &str, container: &str) -> Result<(), AzureError> {
        let blob_rel_paths = self.db.delete_container(account, container).await?;
        for rel in blob_rel_paths {
            let abs = self.storage_root.join(&rel);
            let _ = fs_io::remove_file_if_exists(&abs).await;
        }
        let dir = path_layout::container_root(&self.storage_root, account, container);
        let _ = tokio::fs::remove_dir_all(&dir).await;
        Ok(())
    }

    async fn get_container_properties(
        &self,
        account: &str,
        container: &str,
    ) -> Result<ContainerProps, AzureError> {
        self.db
            .require_container(account, container)
            .await
            .map(container_row_to_props)
    }

    async fn set_container_metadata(
        &self,
        account: &str,
        container: &str,
        metadata: HashMap<String, String>,
    ) -> Result<ContainerProps, AzureError> {
        self.db
            .set_container_metadata(account, container, metadata)
            .await
            .map(container_row_to_props)
    }

    async fn set_container_acl(
        &self,
        account: &str,
        container: &str,
        public_access: PublicAccessLevel,
    ) -> Result<ContainerProps, AzureError> {
        self.db
            .set_container_acl(account, container, public_access)
            .await
            .map(container_row_to_props)
    }

    async fn list_containers(
        &self,
        account: &str,
        prefix: Option<&str>,
        marker: Option<&str>,
        max_results: u32,
    ) -> Result<ContainerListPage, AzureError> {
        let (rows, next_marker) = self
            .db
            .list_containers(account, prefix, marker, max_results)
            .await?;
        Ok(ContainerListPage {
            containers: rows.into_iter().map(container_row_to_props).collect(),
            next_marker,
        })
    }

    async fn list_blobs(
        &self,
        account: &str,
        container: &str,
        prefix: Option<&str>,
        delimiter: Option<&str>,
        marker: Option<&str>,
        max_results: u32,
    ) -> Result<BlobListPage, AzureError> {
        let (rows, next_marker) = self
            .db
            .list_blobs(account, container, prefix, marker, max_results)
            .await?;

        // Emulate `delimiter`-based virtual-directory grouping (e.g. "/"): any
        // blob name that contains the delimiter after the prefix is folded into
        // a single `BlobPrefix` entry instead of a `Blob` entry.
        let mut blobs = Vec::new();
        let mut prefixes: Vec<String> = Vec::new();
        let prefix_str = prefix.unwrap_or("");

        for row in rows {
            if let Some(delim) = delimiter {
                let rest = &row.name[prefix_str.len().min(row.name.len())..];
                if let Some(idx) = rest.find(delim) {
                    let group_prefix = format!("{}{}", prefix_str, &rest[..idx + delim.len()]);
                    if !prefixes.contains(&group_prefix) {
                        prefixes.push(group_prefix);
                    }
                    continue;
                }
            }
            blobs.push(BlobListEntry {
                props: blob_row_to_props(row),
            });
        }

        Ok(BlobListPage {
            blobs,
            blob_prefixes: prefixes,
            next_marker,
        })
    }

    async fn put_blob(
        &self,
        account: &str,
        container: &str,
        blob: &str,
        body: ByteStream,
        content_type: Option<String>,
        content_md5_header: Option<String>,
        metadata: HashMap<String, String>,
    ) -> Result<BlobProps, AzureError> {
        self.db.require_container(account, container).await?;

        let (abs_path, rel_path) = self.blob_paths(account, container, blob);
        let tmp = self.tmp_dir(account);
        let (size, md5_hex) = fs_io::write_stream_atomic(&tmp, &abs_path, body)
            .await
            .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;

        let md5_b64 = md5_hex_to_base64(&md5_hex);
        if let Some(expected) = &content_md5_header {
            if Some(expected) != md5_b64.as_ref() {
                let _ = fs_io::remove_file_if_exists(&abs_path).await;
                return Err(AzureErrorCode::Md5Mismatch.into());
            }
        }

        let row = self
            .db
            .upsert_blob_single_shot(
                account,
                container,
                blob,
                &rel_path,
                size,
                content_type,
                md5_b64,
                metadata,
            )
            .await?;
        Ok(blob_row_to_props(row))
    }

    async fn get_blob(
        &self,
        account: &str,
        container: &str,
        blob: &str,
        range: Option<(u64, u64)>,
    ) -> Result<BlobPayload, AzureError> {
        let row = self.db.require_blob(account, container, blob).await?;
        let abs_path = self.storage_root.join(&row.storage_path);
        let total_len = row.content_length;

        let resolved_range = match range {
            Some((start, end)) => {
                let end = end.min(total_len.saturating_sub(1));
                if start > end || start >= total_len {
                    return Err(AzureErrorCode::InvalidRange.into());
                }
                Some(ByteRange { start, end })
            }
            None => None,
        };

        let mut file = tokio::fs::File::open(&abs_path)
            .await
            .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;

        let take_len = if let Some(r) = resolved_range {
            file.seek(std::io::SeekFrom::Start(r.start))
                .await
                .map_err(|e| {
                    AzureError::with_message(AzureErrorCode::InternalError, e.to_string())
                })?;
            r.end - r.start + 1
        } else {
            total_len
        };

        let reader = Box::pin(file.take(take_len));

        Ok(BlobPayload {
            props: blob_row_to_props(row),
            total_len,
            range: resolved_range,
            reader,
        })
    }

    async fn get_blob_properties(
        &self,
        account: &str,
        container: &str,
        blob: &str,
    ) -> Result<BlobProps, AzureError> {
        self.db
            .require_blob(account, container, blob)
            .await
            .map(blob_row_to_props)
    }

    async fn set_blob_properties(
        &self,
        account: &str,
        container: &str,
        blob: &str,
        patch: BlobPropertiesPatch,
    ) -> Result<BlobProps, AzureError> {
        self.db
            .set_blob_properties(
                account,
                container,
                blob,
                patch.content_type,
                patch.content_md5,
                patch.content_encoding,
                patch.content_language,
                patch.cache_control,
                patch.content_disposition,
            )
            .await
            .map(blob_row_to_props)
    }

    async fn set_blob_metadata(
        &self,
        account: &str,
        container: &str,
        blob: &str,
        metadata: HashMap<String, String>,
    ) -> Result<BlobProps, AzureError> {
        self.db
            .set_blob_metadata(account, container, blob, metadata)
            .await
            .map(blob_row_to_props)
    }

    async fn delete_blob(
        &self,
        account: &str,
        container: &str,
        blob: &str,
    ) -> Result<(), AzureError> {
        let rel_path = self.db.delete_blob(account, container, blob).await?;
        let abs_path = self.storage_root.join(&rel_path);
        let _ = fs_io::remove_file_if_exists(&abs_path).await;
        Ok(())
    }

    async fn put_block(
        &self,
        account: &str,
        container: &str,
        blob: &str,
        block_id: &str,
        body: ByteStream,
    ) -> Result<String, AzureError> {
        self.db.require_container(account, container).await?;
        let (abs_path, rel_path) = self.staged_block_path(account, container, blob, block_id);
        let tmp = self.tmp_dir(account);
        let (size, md5_hex) = fs_io::write_stream_atomic(&tmp, &abs_path, body)
            .await
            .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;
        let md5_b64 = md5_hex_to_base64(&md5_hex);
        self.db
            .stage_block(
                account,
                container,
                blob,
                block_id,
                size,
                md5_b64.clone(),
                &rel_path,
            )
            .await?;
        Ok(md5_b64.unwrap_or_default())
    }

    async fn put_block_list(
        &self,
        account: &str,
        container: &str,
        blob: &str,
        blocks: Vec<BlockRef>,
        content_type: Option<String>,
        metadata: HashMap<String, String>,
    ) -> Result<BlobProps, AzureError> {
        self.db.require_container(account, container).await?;

        let committed = self.db.get_committed_blocks(account, container, blob).await;
        let committed_map: HashMap<&str, u64> = committed
            .iter()
            .map(|b| (b.block_id.as_str(), b.block_size))
            .collect();

        let mut resolved: Vec<(String, u64, PathBuf)> = Vec::with_capacity(blocks.len());
        for block_ref in &blocks {
            let staged = self
                .db
                .get_staged_block(account, container, blob, &block_ref.block_id)
                .await;

            let (size, abs_path) = match block_ref.kind {
                BlockRefKind::Uncommitted => match staged {
                    Some(s) => (s.block_size, self.storage_root.join(&s.storage_path)),
                    None => return Err(AzureErrorCode::InvalidBlockList.into()),
                },
                BlockRefKind::Committed => match committed_map.get(block_ref.block_id.as_str()) {
                    Some(&size) => {
                        // Existing committed blocks are re-read from the current
                        // assembled blob's staged copy if still present, else
                        // treated as already part of the current blob content.
                        // Re-committing without re-upload is rare from real
                        // clients (Storage Explorer/azcopy always send `Latest`),
                        // so v1 requires the block still be staged.
                        match staged {
                            Some(s) => (size, self.storage_root.join(&s.storage_path)),
                            None => return Err(AzureErrorCode::InvalidBlockList.into()),
                        }
                    }
                    None => return Err(AzureErrorCode::InvalidBlockList.into()),
                },
                BlockRefKind::Latest => {
                    if let Some(s) = staged {
                        (s.block_size, self.storage_root.join(&s.storage_path))
                    } else if let Some(&size) = committed_map.get(block_ref.block_id.as_str()) {
                        (size, PathBuf::new())
                    } else {
                        return Err(AzureErrorCode::InvalidBlockList.into());
                    }
                }
            };
            resolved.push((block_ref.block_id.clone(), size, abs_path));
        }

        let (abs_final, rel_final) = self.blob_paths(account, container, blob);
        let tmp = self.tmp_dir(account);
        let part_paths: Vec<PathBuf> = resolved.iter().map(|(_, _, p)| p.clone()).collect();
        if part_paths.iter().any(|p| p.as_os_str().is_empty()) {
            // A `Committed`/`Latest` reference pointed at a block that's part of
            // the existing blob but no longer staged; v1 doesn't keep committed
            // block bytes addressable independently, so this combination isn't
            // supported (documented known gap - out of scope for v1).
            return Err(AzureErrorCode::InvalidBlockList.into());
        }

        let (total_size, md5_hex) = fs_io::concatenate_files_atomic(&tmp, &abs_final, &part_paths)
            .await
            .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;
        let md5_b64 = md5_hex_to_base64(&md5_hex).unwrap_or_default();

        let resolved_for_db: Vec<(String, u64)> = resolved
            .iter()
            .map(|(id, size, _)| (id.clone(), *size))
            .collect();

        let row = self
            .db
            .commit_block_list(
                account,
                container,
                blob,
                &resolved_for_db,
                &rel_final,
                total_size,
                md5_b64,
                content_type,
                metadata,
            )
            .await?;

        // Best-effort cleanup of now-committed staged block files.
        for (_, _, path) in &resolved {
            if !path.as_os_str().is_empty() {
                let _ = fs_io::remove_file_if_exists(path).await;
            }
        }

        Ok(blob_row_to_props(row))
    }

    async fn get_block_list(
        &self,
        account: &str,
        container: &str,
        blob: &str,
    ) -> Result<BlockListResult, AzureError> {
        let committed = self.db.get_committed_blocks(account, container, blob).await;
        let uncommitted = self
            .db
            .get_uncommitted_blocks(account, container, blob)
            .await;
        Ok(BlockListResult {
            committed: committed
                .into_iter()
                .map(|b| CommittedBlock {
                    block_id: b.block_id,
                    size: b.block_size,
                })
                .collect(),
            uncommitted: uncommitted
                .into_iter()
                .map(|b| CommittedBlock {
                    block_id: b.block_id,
                    size: b.block_size,
                })
                .collect(),
        })
    }
}
