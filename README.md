# MertBlobStorage

A self-hosted, production-oriented Rust server implementing the **Azure Blob
Storage REST API** (core surface: containers + block blobs), so real Azure
tooling — Azure Storage Explorer, `azcopy`, and the official Azure SDKs — can
point at it via a custom endpoint. Think "self-hosted Azurite, but built for
durability rather than local development."

See [docs/design.md](docs/design.md) for the full architecture writeup.

## Status: v1 (core blob service)

Implemented:
- Container CRUD, metadata, ACL (public access level), List Containers, List Blobs (prefix/delimiter/marker pagination)
- Block blob: Put Blob, Get Blob (with `Range` support), Head Blob, Delete Blob, Put Block, Put Block List, Get Block List, Set/Get Properties & Metadata
- Auth: Shared Key (HMAC-SHA256), SAS tokens (account-key-signed service SAS), anonymous public access, Azure AD/OAuth2 bearer (JWT vs. a configurable OIDC issuer)
- Durability: atomic write-then-rename with fsync, SQLite (WAL) metadata store
- Streaming request/response bodies (no whole-blob buffering)

Explicitly out of scope for v1 (see `docs/design.md` for the plan to add them):
append/page blobs, snapshots, leases, copy-blob, blob tiers, batch operations,
stored access policies.

## Quick start

```sh
# 1. Build
cargo build --release

# 2. Configure (copy and edit; see comments in the file)
cp config/default.toml config/my-config.toml

# 3. Run
./target/release/mertblobstorage --config config/my-config.toml
```

If you don't configure any `[[accounts]]`, the server generates a throwaway
`devstoreaccount1` account with a random key on first startup and logs it —
convenient for trying things out, but add a real entry to your config for
anything that needs to survive a restart.

### Docker

```sh
docker build -t mertblobstorage .
docker run -d --name mertblobstorage \
  -p 8443:8443 \
  -v mbs-data:/app/data \
  mertblobstorage
```

Or with Compose:

```sh
docker compose up -d
```

The image runs as a non-root user, exposes port `8443` (plaintext by
default — see `config/default.toml` to enable TLS by mounting a cert/key and
setting `server.tls_enabled = true`), and stores everything under `/app/data`,
which you should mount as a volume. Override the built-in config by mounting
your own file over `/app/config/default.toml` (see the commented-out line in
`docker-compose.yml`) or by setting `MBS_*` environment variables — e.g.
`-e MBS_ACCOUNTS__0__NAME=myaccount -e MBS_ACCOUNTS__0__KEY1=<base64 key>`.

### Connecting Azure Storage Explorer

Use "Connect to a resource using a connection string" (or "Attach to a local
emulator" style custom endpoint) with:

```
DefaultEndpointsProtocol=http;AccountName=<account>;AccountKey=<key1>;BlobEndpoint=http://<host>:<port>/<account>;
```

### Connecting azcopy / SDKs

Any Azure Blob SDK client configured with a Shared Key, SAS, or bearer
credential and a custom `BlobEndpoint` pointing at this server's
`http(s)://host:port/{account}` will work the same way it would against real
Azure Blob Storage.

## Development

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```

## License

MIT
