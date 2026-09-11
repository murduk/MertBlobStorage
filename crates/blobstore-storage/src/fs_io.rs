//! Durable, crash-safe filesystem primitives: write-to-temp-then-rename, with
//! fsync on both the file and its parent directory (POSIX requires syncing the
//! directory entry too for a rename to be durable across a crash).

use std::path::{Path, PathBuf};

use futures::StreamExt;
use md5::{Digest as Md5Digest, Md5};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use crate::types::ByteStream;

fn unique_tmp_path(tmp_dir: &Path) -> PathBuf {
    tmp_dir.join(format!("{}.tmp", uuid_like()))
}

/// Cheap unique-id generator (avoids pulling `uuid` into this crate's hot path
/// just for temp file names); collisions are astronomically unlikely and would
/// only matter within the same process between two in-flight writes.
fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tid = std::thread::current().id();
    format!("{nanos:x}-{tid:?}").replace(['(', ')', 'T', 'h', 'r', 'e', 'a', 'd', 'I', 'd'], "")
}

async fn fsync_dir(dir: &Path) -> std::io::Result<()> {
    // Directory fsync is a no-op on some platforms' std wrappers but is
    // supported on Linux/macOS via opening the directory as a read-only file.
    match File::open(dir).await {
        Ok(f) => f.sync_all().await,
        Err(e) => {
            // Best-effort: some platforms disallow opening directories this way.
            tracing::debug!("could not fsync directory {}: {}", dir.display(), e);
            Ok(())
        }
    }
}

/// Streams `body` into a temp file under `tmp_dir`, fsyncs it, then atomically
/// renames it onto `final_path` (creating `final_path`'s parent directory if
/// needed) and fsyncs that parent directory. Returns `(size, md5_hex)`.
///
/// `tmp_dir` and `final_path` MUST be on the same filesystem for the rename to
/// be atomic rather than a cross-device copy.
pub async fn write_stream_atomic(
    tmp_dir: &Path,
    final_path: &Path,
    mut body: ByteStream,
) -> std::io::Result<(u64, String)> {
    tokio::fs::create_dir_all(tmp_dir).await?;
    if let Some(parent) = final_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let tmp_path = unique_tmp_path(tmp_dir);
    let mut file = File::create(&tmp_path).await?;
    let mut hasher = Md5::new();
    let mut total: u64 = 0;

    while let Some(chunk) = body.next().await {
        let chunk = chunk?;
        hasher.update(&chunk);
        total += chunk.len() as u64;
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    file.sync_all().await?;
    drop(file);

    tokio::fs::rename(&tmp_path, final_path).await?;
    if let Some(parent) = final_path.parent() {
        fsync_dir(parent).await?;
    }

    let md5_hex = hex::encode(hasher.finalize());
    Ok((total, md5_hex))
}

/// Concatenates the files at `part_paths`, in order, into a new file at
/// `final_path`, using the same atomic write-then-rename protocol. Used to
/// assemble a block blob from its staged/committed blocks on Put Block List.
pub async fn concatenate_files_atomic(
    tmp_dir: &Path,
    final_path: &Path,
    part_paths: &[PathBuf],
) -> std::io::Result<(u64, String)> {
    tokio::fs::create_dir_all(tmp_dir).await?;
    if let Some(parent) = final_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let tmp_path = unique_tmp_path(tmp_dir);
    let mut out = File::create(&tmp_path).await?;
    let mut hasher = Md5::new();
    let mut total: u64 = 0;

    for part in part_paths {
        let mut input = File::open(part).await?;
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            use tokio::io::AsyncReadExt;
            let n = input.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            total += n as u64;
            out.write_all(&buf[..n]).await?;
        }
    }
    out.flush().await?;
    out.sync_all().await?;
    drop(out);

    tokio::fs::rename(&tmp_path, final_path).await?;
    if let Some(parent) = final_path.parent() {
        fsync_dir(parent).await?;
    }

    let md5_hex = hex::encode(hasher.finalize());
    Ok((total, md5_hex))
}

/// Best-effort delete; missing files are not an error (idempotent cleanup).
pub async fn remove_file_if_exists(path: &Path) -> std::io::Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
