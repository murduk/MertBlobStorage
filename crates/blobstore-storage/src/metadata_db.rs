//! SQLite-backed metadata store. Uses runtime-checked `sqlx::query` (not the
//! `query!` macro) so the workspace builds without a live DATABASE_URL / cached
//! query metadata at compile time.

use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

use blobstore_core::{AzureError, AzureErrorCode, PublicAccessLevel};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

pub struct MetadataDb {
    pub(crate) pool: SqlitePool,
}

#[derive(Debug, Clone)]
pub struct ContainerRow {
    pub name: String,
    pub public_access_level: PublicAccessLevel,
    pub metadata: HashMap<String, String>,
    pub etag: String,
    pub last_modified: String,
}

#[derive(Debug, Clone)]
pub struct BlobRow {
    pub name: String,
    pub blob_type: String,
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
    pub storage_path: String,
}

#[derive(Debug, Clone)]
pub struct StagedBlockRow {
    pub block_size: u64,
    pub storage_path: String,
}

#[derive(Debug, Clone)]
pub struct CommittedBlockRow {
    pub block_id: String,
    pub block_size: u64,
}

fn parse_metadata(json: &str) -> HashMap<String, String> {
    serde_json::from_str(json).unwrap_or_default()
}

fn dump_metadata(m: &HashMap<String, String>) -> String {
    serde_json::to_string(m).unwrap_or_else(|_| "{}".to_string())
}

fn now_rfc1123() -> String {
    blobstore_core::format_http_date(time::OffsetDateTime::now_utc())
}

fn now_iso() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

impl MetadataDb {
    pub async fn connect(db_path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.display()))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Full)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(16)
            .connect_with(opts)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub async fn connect_in_memory() -> anyhow::Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    // --- Accounts ---

    pub async fn ensure_account(&self, account: &str, key1: &str, key2: Option<&str>) {
        let _ = sqlx::query(
            "INSERT INTO accounts (account_name, key1_base64, key2_base64, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(account_name) DO UPDATE SET key1_base64 = excluded.key1_base64, key2_base64 = excluded.key2_base64",
        )
        .bind(account)
        .bind(key1)
        .bind(key2)
        .bind(now_iso())
        .execute(&self.pool)
        .await;
    }

    pub async fn account_keys(&self, account: &str) -> Option<(String, Option<String>)> {
        let row =
            sqlx::query("SELECT key1_base64, key2_base64 FROM accounts WHERE account_name = ?1")
                .bind(account)
                .fetch_optional(&self.pool)
                .await
                .ok()??;
        Some((row.get("key1_base64"), row.get("key2_base64")))
    }

    // --- Containers ---

    pub async fn create_container(
        &self,
        account: &str,
        container: &str,
        public_access: PublicAccessLevel,
        metadata: HashMap<String, String>,
    ) -> Result<ContainerRow, AzureError> {
        let etag = blobstore_core::generate_etag();
        let last_modified = now_rfc1123();
        let res = sqlx::query(
            "INSERT INTO containers (account_name, container_name, public_access_level, metadata_json, etag, last_modified, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(account)
        .bind(container)
        .bind(public_access.as_db_str())
        .bind(dump_metadata(&metadata))
        .bind(&etag)
        .bind(&last_modified)
        .bind(now_iso())
        .execute(&self.pool)
        .await;

        match res {
            Ok(_) => Ok(ContainerRow {
                name: container.to_string(),
                public_access_level: public_access,
                metadata,
                etag,
                last_modified,
            }),
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                Err(AzureErrorCode::ContainerAlreadyExists.into())
            }
            Err(e) => Err(AzureError::with_message(
                AzureErrorCode::InternalError,
                e.to_string(),
            )),
        }
    }

    pub async fn get_container(&self, account: &str, container: &str) -> Option<ContainerRow> {
        let row = sqlx::query(
            "SELECT container_name, public_access_level, metadata_json, etag, last_modified
             FROM containers WHERE account_name = ?1 AND container_name = ?2",
        )
        .bind(account)
        .bind(container)
        .fetch_optional(&self.pool)
        .await
        .ok()??;

        Some(ContainerRow {
            name: row.get("container_name"),
            public_access_level: PublicAccessLevel::from_db_str(row.get("public_access_level")),
            metadata: parse_metadata(row.get("metadata_json")),
            etag: row.get("etag"),
            last_modified: row.get("last_modified"),
        })
    }

    pub async fn require_container(
        &self,
        account: &str,
        container: &str,
    ) -> Result<ContainerRow, AzureError> {
        self.get_container(account, container)
            .await
            .ok_or_else(|| AzureErrorCode::ContainerNotFound.into())
    }

    pub async fn delete_container(
        &self,
        account: &str,
        container: &str,
    ) -> Result<Vec<String>, AzureError> {
        self.require_container(account, container).await?;

        let mut tx =
            self.pool.begin().await.map_err(|e| {
                AzureError::with_message(AzureErrorCode::InternalError, e.to_string())
            })?;

        let blob_paths: Vec<String> = sqlx::query(
            "SELECT storage_path FROM blobs WHERE account_name = ?1 AND container_name = ?2",
        )
        .bind(account)
        .bind(container)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?
        .into_iter()
        .map(|r| r.get::<String, _>("storage_path"))
        .collect();

        for tbl in ["committed_blocks", "staged_blocks", "blobs"] {
            sqlx::query(&format!(
                "DELETE FROM {tbl} WHERE account_name = ?1 AND container_name = ?2"
            ))
            .bind(account)
            .bind(container)
            .execute(&mut *tx)
            .await
            .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;
        }

        sqlx::query("DELETE FROM containers WHERE account_name = ?1 AND container_name = ?2")
            .bind(account)
            .bind(container)
            .execute(&mut *tx)
            .await
            .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;

        Ok(blob_paths)
    }

    pub async fn set_container_metadata(
        &self,
        account: &str,
        container: &str,
        metadata: HashMap<String, String>,
    ) -> Result<ContainerRow, AzureError> {
        self.require_container(account, container).await?;
        let etag = blobstore_core::generate_etag();
        let last_modified = now_rfc1123();
        sqlx::query(
            "UPDATE containers SET metadata_json = ?1, etag = ?2, last_modified = ?3
             WHERE account_name = ?4 AND container_name = ?5",
        )
        .bind(dump_metadata(&metadata))
        .bind(&etag)
        .bind(&last_modified)
        .bind(account)
        .bind(container)
        .execute(&self.pool)
        .await
        .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;

        self.require_container(account, container).await
    }

    pub async fn set_container_acl(
        &self,
        account: &str,
        container: &str,
        public_access: PublicAccessLevel,
    ) -> Result<ContainerRow, AzureError> {
        self.require_container(account, container).await?;
        let etag = blobstore_core::generate_etag();
        let last_modified = now_rfc1123();
        sqlx::query(
            "UPDATE containers SET public_access_level = ?1, etag = ?2, last_modified = ?3
             WHERE account_name = ?4 AND container_name = ?5",
        )
        .bind(public_access.as_db_str())
        .bind(&etag)
        .bind(&last_modified)
        .bind(account)
        .bind(container)
        .execute(&self.pool)
        .await
        .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;

        self.require_container(account, container).await
    }

    pub async fn list_containers(
        &self,
        account: &str,
        prefix: Option<&str>,
        marker: Option<&str>,
        max_results: u32,
    ) -> Result<(Vec<ContainerRow>, Option<String>), AzureError> {
        let prefix = prefix.unwrap_or("");
        let marker = marker.unwrap_or("");
        let rows = sqlx::query(
            "SELECT container_name, public_access_level, metadata_json, etag, last_modified
             FROM containers
             WHERE account_name = ?1 AND container_name LIKE ?2 || '%' AND container_name > ?3
             ORDER BY container_name
             LIMIT ?4",
        )
        .bind(account)
        .bind(prefix)
        .bind(marker)
        .bind((max_results + 1) as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;

        let mut containers: Vec<ContainerRow> = rows
            .into_iter()
            .map(|row| ContainerRow {
                name: row.get("container_name"),
                public_access_level: PublicAccessLevel::from_db_str(row.get("public_access_level")),
                metadata: parse_metadata(row.get("metadata_json")),
                etag: row.get("etag"),
                last_modified: row.get("last_modified"),
            })
            .collect();

        let next_marker = if containers.len() > max_results as usize {
            containers.truncate(max_results as usize);
            containers.last().map(|c| c.name.clone())
        } else {
            None
        };

        Ok((containers, next_marker))
    }

    // --- Blobs ---

    fn row_to_blob(row: &sqlx::sqlite::SqliteRow) -> BlobRow {
        BlobRow {
            name: row.get("blob_name"),
            blob_type: row.get("blob_type"),
            content_length: row.get::<i64, _>("content_length") as u64,
            content_type: row.get("content_type"),
            content_md5: row.get("content_md5"),
            content_encoding: row.get("content_encoding"),
            content_language: row.get("content_language"),
            cache_control: row.get("cache_control"),
            content_disposition: row.get("content_disposition"),
            metadata: parse_metadata(row.get("metadata_json")),
            etag: row.get("etag"),
            last_modified: row.get("last_modified"),
            storage_path: row.get("storage_path"),
        }
    }

    pub async fn get_blob(&self, account: &str, container: &str, blob: &str) -> Option<BlobRow> {
        let row = sqlx::query(
            "SELECT * FROM blobs WHERE account_name = ?1 AND container_name = ?2 AND blob_name = ?3 AND committed = 1",
        )
        .bind(account)
        .bind(container)
        .bind(blob)
        .fetch_optional(&self.pool)
        .await
        .ok()??;
        Some(Self::row_to_blob(&row))
    }

    pub async fn require_blob(
        &self,
        account: &str,
        container: &str,
        blob: &str,
    ) -> Result<BlobRow, AzureError> {
        self.get_blob(account, container, blob)
            .await
            .ok_or_else(|| AzureErrorCode::BlobNotFound.into())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_blob_single_shot(
        &self,
        account: &str,
        container: &str,
        blob: &str,
        storage_path: &str,
        content_length: u64,
        content_type: Option<String>,
        content_md5: Option<String>,
        metadata: HashMap<String, String>,
    ) -> Result<BlobRow, AzureError> {
        self.require_container(account, container).await?;
        let etag = blobstore_core::generate_etag();
        let last_modified = now_rfc1123();
        sqlx::query(
            "INSERT INTO blobs (account_name, container_name, blob_name, blob_type, content_length,
                content_type, content_md5, metadata_json, etag, last_modified, created_at, committed, storage_path)
             VALUES (?1, ?2, ?3, 'BlockBlob', ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, ?11)
             ON CONFLICT(account_name, container_name, blob_name) DO UPDATE SET
                content_length = excluded.content_length,
                content_type = excluded.content_type,
                content_md5 = excluded.content_md5,
                metadata_json = excluded.metadata_json,
                etag = excluded.etag,
                last_modified = excluded.last_modified,
                committed = 1,
                storage_path = excluded.storage_path",
        )
        .bind(account)
        .bind(container)
        .bind(blob)
        .bind(content_length as i64)
        .bind(&content_type)
        .bind(&content_md5)
        .bind(dump_metadata(&metadata))
        .bind(&etag)
        .bind(&last_modified)
        .bind(now_iso())
        .bind(storage_path)
        .execute(&self.pool)
        .await
        .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;

        self.require_blob(account, container, blob).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn set_blob_properties(
        &self,
        account: &str,
        container: &str,
        blob: &str,
        content_type: Option<String>,
        content_md5: Option<String>,
        content_encoding: Option<String>,
        content_language: Option<String>,
        cache_control: Option<String>,
        content_disposition: Option<String>,
    ) -> Result<BlobRow, AzureError> {
        self.require_blob(account, container, blob).await?;
        let etag = blobstore_core::generate_etag();
        let last_modified = now_rfc1123();
        sqlx::query(
            "UPDATE blobs SET content_type = ?1, content_md5 = ?2, content_encoding = ?3,
                content_language = ?4, cache_control = ?5, content_disposition = ?6,
                etag = ?7, last_modified = ?8
             WHERE account_name = ?9 AND container_name = ?10 AND blob_name = ?11",
        )
        .bind(content_type)
        .bind(content_md5)
        .bind(content_encoding)
        .bind(content_language)
        .bind(cache_control)
        .bind(content_disposition)
        .bind(&etag)
        .bind(&last_modified)
        .bind(account)
        .bind(container)
        .bind(blob)
        .execute(&self.pool)
        .await
        .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;

        self.require_blob(account, container, blob).await
    }

    pub async fn set_blob_metadata(
        &self,
        account: &str,
        container: &str,
        blob: &str,
        metadata: HashMap<String, String>,
    ) -> Result<BlobRow, AzureError> {
        self.require_blob(account, container, blob).await?;
        let etag = blobstore_core::generate_etag();
        let last_modified = now_rfc1123();
        sqlx::query(
            "UPDATE blobs SET metadata_json = ?1, etag = ?2, last_modified = ?3
             WHERE account_name = ?4 AND container_name = ?5 AND blob_name = ?6",
        )
        .bind(dump_metadata(&metadata))
        .bind(&etag)
        .bind(&last_modified)
        .bind(account)
        .bind(container)
        .bind(blob)
        .execute(&self.pool)
        .await
        .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;

        self.require_blob(account, container, blob).await
    }

    pub async fn delete_blob(
        &self,
        account: &str,
        container: &str,
        blob: &str,
    ) -> Result<String, AzureError> {
        let row = self.require_blob(account, container, blob).await?;
        sqlx::query(
            "DELETE FROM blobs WHERE account_name = ?1 AND container_name = ?2 AND blob_name = ?3",
        )
        .bind(account)
        .bind(container)
        .bind(blob)
        .execute(&self.pool)
        .await
        .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;
        for tbl in ["committed_blocks", "staged_blocks"] {
            let _ = sqlx::query(&format!(
                "DELETE FROM {tbl} WHERE account_name = ?1 AND container_name = ?2 AND blob_name = ?3"
            ))
            .bind(account)
            .bind(container)
            .bind(blob)
            .execute(&self.pool)
            .await;
        }
        Ok(row.storage_path)
    }

    pub async fn list_blobs(
        &self,
        account: &str,
        container: &str,
        prefix: Option<&str>,
        marker: Option<&str>,
        max_results: u32,
    ) -> Result<(Vec<BlobRow>, Option<String>), AzureError> {
        self.require_container(account, container).await?;
        let prefix = prefix.unwrap_or("");
        let marker = marker.unwrap_or("");
        let rows = sqlx::query(
            "SELECT * FROM blobs
             WHERE account_name = ?1 AND container_name = ?2 AND committed = 1
                AND blob_name LIKE ?3 || '%' AND blob_name > ?4
             ORDER BY blob_name
             LIMIT ?5",
        )
        .bind(account)
        .bind(container)
        .bind(prefix)
        .bind(marker)
        .bind((max_results as i64) + 1)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;

        let mut blobs: Vec<BlobRow> = rows.iter().map(Self::row_to_blob).collect();
        let next_marker = if blobs.len() > max_results as usize {
            blobs.truncate(max_results as usize);
            blobs.last().map(|b| b.name.clone())
        } else {
            None
        };
        Ok((blobs, next_marker))
    }

    // --- Blocks ---

    #[allow(clippy::too_many_arguments)]
    pub async fn stage_block(
        &self,
        account: &str,
        container: &str,
        blob: &str,
        block_id: &str,
        block_size: u64,
        md5: Option<String>,
        storage_path: &str,
    ) -> Result<(), AzureError> {
        sqlx::query(
            "INSERT INTO staged_blocks (account_name, container_name, blob_name, block_id, block_size, md5, storage_path, staged_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(account_name, container_name, blob_name, block_id) DO UPDATE SET
                block_size = excluded.block_size, md5 = excluded.md5, storage_path = excluded.storage_path,
                staged_at = excluded.staged_at",
        )
        .bind(account)
        .bind(container)
        .bind(blob)
        .bind(block_id)
        .bind(block_size as i64)
        .bind(md5)
        .bind(storage_path)
        .bind(now_iso())
        .execute(&self.pool)
        .await
        .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;
        Ok(())
    }

    pub async fn get_staged_block(
        &self,
        account: &str,
        container: &str,
        blob: &str,
        block_id: &str,
    ) -> Option<StagedBlockRow> {
        let row = sqlx::query(
            "SELECT block_size, storage_path FROM staged_blocks
             WHERE account_name = ?1 AND container_name = ?2 AND blob_name = ?3 AND block_id = ?4",
        )
        .bind(account)
        .bind(container)
        .bind(blob)
        .bind(block_id)
        .fetch_optional(&self.pool)
        .await
        .ok()??;
        Some(StagedBlockRow {
            block_size: row.get::<i64, _>("block_size") as u64,
            storage_path: row.get("storage_path"),
        })
    }

    pub async fn get_committed_blocks(
        &self,
        account: &str,
        container: &str,
        blob: &str,
    ) -> Vec<CommittedBlockRow> {
        sqlx::query(
            "SELECT block_id, block_size FROM committed_blocks
             WHERE account_name = ?1 AND container_name = ?2 AND blob_name = ?3
             ORDER BY block_index",
        )
        .bind(account)
        .bind(container)
        .bind(blob)
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|row| CommittedBlockRow {
            block_id: row.get("block_id"),
            block_size: row.get::<i64, _>("block_size") as u64,
        })
        .collect()
    }

    pub async fn get_uncommitted_blocks(
        &self,
        account: &str,
        container: &str,
        blob: &str,
    ) -> Vec<CommittedBlockRow> {
        sqlx::query(
            "SELECT block_id, block_size FROM staged_blocks
             WHERE account_name = ?1 AND container_name = ?2 AND blob_name = ?3
             ORDER BY staged_at",
        )
        .bind(account)
        .bind(container)
        .bind(blob)
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|row| CommittedBlockRow {
            block_id: row.get("block_id"),
            block_size: row.get::<i64, _>("block_size") as u64,
        })
        .collect()
    }

    /// Commits a resolved block list (already validated against staged/committed
    /// blocks by the caller) as the new content of `blob`, in one transaction:
    /// replace `committed_blocks`, upsert the `blobs` row, and clear consumed
    /// `staged_blocks`. The final bytes must already have been written to
    /// `storage_path` by the caller (via `fs_io::concatenate_files_atomic`)
    /// *before* this is called, so the DB is only updated once the durable
    /// write has succeeded.
    #[allow(clippy::too_many_arguments)]
    pub async fn commit_block_list(
        &self,
        account: &str,
        container: &str,
        blob: &str,
        resolved: &[(String, u64)],
        storage_path: &str,
        total_size: u64,
        content_md5: String,
        content_type: Option<String>,
        metadata: HashMap<String, String>,
    ) -> Result<BlobRow, AzureError> {
        self.require_container(account, container).await?;
        let mut tx =
            self.pool.begin().await.map_err(|e| {
                AzureError::with_message(AzureErrorCode::InternalError, e.to_string())
            })?;

        sqlx::query(
            "DELETE FROM committed_blocks WHERE account_name = ?1 AND container_name = ?2 AND blob_name = ?3",
        )
        .bind(account)
        .bind(container)
        .bind(blob)
        .execute(&mut *tx)
        .await
        .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;

        for (idx, (block_id, size)) in resolved.iter().enumerate() {
            sqlx::query(
                "INSERT INTO committed_blocks (account_name, container_name, blob_name, block_index, block_id, block_size)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(account)
            .bind(container)
            .bind(blob)
            .bind(idx as i64)
            .bind(block_id)
            .bind(*size as i64)
            .execute(&mut *tx)
            .await
            .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;
        }

        let etag = blobstore_core::generate_etag();
        let last_modified = now_rfc1123();
        sqlx::query(
            "INSERT INTO blobs (account_name, container_name, blob_name, blob_type, content_length,
                content_type, content_md5, metadata_json, etag, last_modified, created_at, committed, storage_path)
             VALUES (?1, ?2, ?3, 'BlockBlob', ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, ?11)
             ON CONFLICT(account_name, container_name, blob_name) DO UPDATE SET
                content_length = excluded.content_length,
                content_type = excluded.content_type,
                content_md5 = excluded.content_md5,
                metadata_json = excluded.metadata_json,
                etag = excluded.etag,
                last_modified = excluded.last_modified,
                committed = 1,
                storage_path = excluded.storage_path",
        )
        .bind(account)
        .bind(container)
        .bind(blob)
        .bind(total_size as i64)
        .bind(&content_type)
        .bind(&content_md5)
        .bind(dump_metadata(&metadata))
        .bind(&etag)
        .bind(&last_modified)
        .bind(now_iso())
        .bind(storage_path)
        .execute(&mut *tx)
        .await
        .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;

        sqlx::query(
            "DELETE FROM staged_blocks WHERE account_name = ?1 AND container_name = ?2 AND blob_name = ?3",
        )
        .bind(account)
        .bind(container)
        .bind(blob)
        .execute(&mut *tx)
        .await
        .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AzureError::with_message(AzureErrorCode::InternalError, e.to_string()))?;

        self.require_blob(account, container, blob).await
    }
}
