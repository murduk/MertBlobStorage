# Design

This mirrors the plan the project was built from. See the crate-level doc
comments (`cargo doc --workspace --open`) for implementation detail; this file
is the durable "why" reference.

## Crates

- `blobstore-core` — domain types, the `AzureErrorCode`/`AzureError` catalog. No HTTP/storage deps.
- `blobstore-storage` — the `StorageBackend` trait plus `FsBackend` (filesystem bytes + SQLite metadata). Swappable backend boundary.
- `blobstore-auth` — pure functions for Shared Key, SAS, anonymous, and Bearer(JWT) verification. No axum dependency.
- `blobstore-xml` — Azure XML wire format: hand-built response bodies (order/casing under our control) plus a `quick_xml`-based parser for Put Block List's order-sensitive request body.
- `blobstore-api` — axum router, one auth middleware (`auth::auth_middleware`) that resolves all four schemes into a single `AuthContext`, and handlers that are auth-scheme-agnostic.
- `blobstore-server` — binary: config loading (figment: defaults → TOML → `MBS_*` env), tracing, TLS (`axum-server` + rustls) or plaintext, graceful shutdown.

## On-disk layout

```
{storage_root}/metadata.sqlite3 (+ -wal/-shm)
{storage_root}/accounts/{account}/containers/{container}/blobs/{sha256(name)[0:2]}/{sha256(name)[2:4]}/{sha256(name)}.bin
{storage_root}/accounts/{account}/containers/{container}/.staging/blocks/{sha256(blob_name)}/{sha256(block_id)}.blk
{storage_root}/accounts/{account}/tmp/                 # same-filesystem scratch for atomic rename
```

Blob/block names are hashed for the on-disk filename (fixed length, filesystem-safe for any Unicode/length Azure allows); the authoritative logical-name → path mapping lives in the `blobs.storage_path` DB column.

## Durability protocol

Every write (Put Blob, Put Block, Put Block List's assembly) goes: stream to a temp file in the same-filesystem `tmp/` dir → `fsync` the file → `rename()` onto the final path → `fsync` the parent directory → commit the DB row(s) in the same or a subsequent transaction. A crash at any point before the rename leaves the previous state (or nothing) intact; a crash after leaves the new state durable. Readers holding an open file descriptor across a rename keep reading the old inode on POSIX filesystems, so GETs are never torn.

## Auth pipeline

One middleware (`blobstore-api::auth::auth_middleware`), precedence: `Authorization: SharedKey`/`SharedKeyLite` → `Authorization: Bearer` → query-string `sig=` (SAS) → anonymous. All four resolve to a common `AuthContext`; handlers never see which scheme authenticated the caller. See doc comments in `crates/blobstore-api/src/auth.rs` and `crates/blobstore-auth/src/*.rs` for the exact canonicalization/verification algorithms.

## Known gaps (tracked for v2+)

- Append blobs, page blobs
- Blob snapshots, leases, copy-blob, blob tiers
- Batch operations (`$batch`)
- Stored access policies (`si=` SAS parameter) — SAS today is always self-contained (permissions/expiry embedded in the token itself)
- Re-committing a previously-committed block via `Committed`/`Latest` in Put Block List without it still being present in the staged-block pool (rare in practice — real clients almost always send `Latest` with freshly-staged blocks)
- Multi-range GET requests (`Range: bytes=0-10,20-30`) — single range only
- Fine-grained RBAC for bearer tokens (a valid token currently grants full account access, same as Shared Key)

## Testing

- Unit tests: `crates/blobstore-auth/src/shared_key_tests.rs` (canonicalization), `crates/blobstore-xml/src/block_list_tests.rs` (order-preserving parse), `crates/blobstore-storage/src/tests.rs` (backend round-trips: container/blob CRUD, block assembly, ranges, listing).
- Manual verification: see README "Connecting Azure Storage Explorer" / "Connecting azcopy / SDKs".
