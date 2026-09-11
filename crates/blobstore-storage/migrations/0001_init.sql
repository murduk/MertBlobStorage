-- Initial metadata schema. See docs/design.md for rationale.

CREATE TABLE IF NOT EXISTS accounts (
    account_name  TEXT PRIMARY KEY,
    key1_base64   TEXT NOT NULL,
    key2_base64   TEXT,
    created_at    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS containers (
    account_name        TEXT NOT NULL REFERENCES accounts(account_name),
    container_name       TEXT NOT NULL,
    public_access_level  TEXT NOT NULL DEFAULT 'off',
    metadata_json         TEXT NOT NULL DEFAULT '{}',
    lease_state             TEXT,
    etag                    TEXT NOT NULL,
    last_modified            TEXT NOT NULL,
    created_at               TEXT NOT NULL,
    deleted                  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (account_name, container_name)
);

CREATE TABLE IF NOT EXISTS blobs (
    account_name          TEXT NOT NULL,
    container_name         TEXT NOT NULL,
    blob_name               TEXT NOT NULL,
    blob_type                TEXT NOT NULL DEFAULT 'BlockBlob',
    content_length           INTEGER NOT NULL DEFAULT 0,
    content_type              TEXT,
    content_md5               TEXT,
    content_encoding          TEXT,
    content_language          TEXT,
    cache_control             TEXT,
    content_disposition       TEXT,
    metadata_json              TEXT NOT NULL DEFAULT '{}',
    etag                       TEXT NOT NULL,
    last_modified              TEXT NOT NULL,
    created_at                 TEXT NOT NULL,
    lease_state                TEXT,
    committed                   INTEGER NOT NULL DEFAULT 1,
    storage_path                TEXT NOT NULL,
    PRIMARY KEY (account_name, container_name, blob_name),
    FOREIGN KEY (account_name, container_name) REFERENCES containers(account_name, container_name)
);

CREATE TABLE IF NOT EXISTS staged_blocks (
    account_name       TEXT NOT NULL,
    container_name      TEXT NOT NULL,
    blob_name            TEXT NOT NULL,
    block_id              TEXT NOT NULL,
    block_size            INTEGER NOT NULL,
    md5                    TEXT,
    storage_path           TEXT NOT NULL,
    staged_at               TEXT NOT NULL,
    PRIMARY KEY (account_name, container_name, blob_name, block_id)
);

CREATE TABLE IF NOT EXISTS committed_blocks (
    account_name       TEXT NOT NULL,
    container_name      TEXT NOT NULL,
    blob_name            TEXT NOT NULL,
    block_index           INTEGER NOT NULL,
    block_id               TEXT NOT NULL,
    block_size             INTEGER NOT NULL,
    PRIMARY KEY (account_name, container_name, blob_name, block_index)
);

CREATE INDEX IF NOT EXISTS idx_blobs_listing
    ON blobs (account_name, container_name, blob_name);
