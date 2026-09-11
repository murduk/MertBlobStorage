//! Maps logical (account, container, blob) names to safe, fixed-length on-disk
//! paths. Blob names in Azure may contain `/`, arbitrary Unicode, and be up to
//! 1024 characters long, none of which are safe to use as literal path
//! components on every filesystem — so we hash the logical name instead and
//! keep the authoritative logical-name -> path mapping in the `blobs.storage_path`
//! DB column. This module only computes *new* paths; existing blobs are looked
//! up by the path already recorded in the DB.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn hash_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

/// Root directory for an account: `{storage_root}/accounts/{account}`.
pub fn account_root(storage_root: &Path, account: &str) -> PathBuf {
    storage_root.join("accounts").join(account)
}

/// Container root: `{account_root}/containers/{container}`.
pub fn container_root(storage_root: &Path, account: &str, container: &str) -> PathBuf {
    account_root(storage_root, account)
        .join("containers")
        .join(container)
}

/// Same-filesystem scratch directory used for atomic write-then-rename, kept
/// under the account root so temp files are always on the same filesystem as
/// their eventual destination (required for `rename()` to be atomic).
#[allow(dead_code)]
pub fn tmp_dir(storage_root: &Path, account: &str) -> PathBuf {
    account_root(storage_root, account).join("tmp")
}

/// Final on-disk path for a committed blob's bytes:
/// `{container_root}/blobs/{h[0:2]}/{h[2:4]}/{h}.bin`.
pub fn blob_rel_path(blob_name: &str) -> PathBuf {
    let h = hash_hex(blob_name);
    PathBuf::from("blobs")
        .join(&h[0..2])
        .join(&h[2..4])
        .join(format!("{h}.bin"))
}

/// Path for one staged (uncommitted) block:
/// `{container_root}/.staging/blocks/{blob_hash}/{block_id_hash}.blk`.
pub fn staged_block_rel_path(blob_name: &str, block_id: &str) -> PathBuf {
    let blob_h = hash_hex(blob_name);
    let block_h = hash_hex(block_id);
    PathBuf::from(".staging")
        .join("blocks")
        .join(blob_h)
        .join(format!("{block_h}.blk"))
}

/// Directory holding all staged blocks for one blob, used for GC/cleanup.
#[allow(dead_code)]
pub fn staged_block_dir(blob_name: &str) -> PathBuf {
    let blob_h = hash_hex(blob_name);
    PathBuf::from(".staging").join("blocks").join(blob_h)
}
