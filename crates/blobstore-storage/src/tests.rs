#[cfg(test)]
mod backend_tests {
    use std::collections::HashMap;

    use bytes::Bytes;
    use futures::stream;

    use crate::backend::StorageBackend;
    use crate::fs_backend::FsBackend;
    use crate::types::{BlockRef, BlockRefKind, ByteStream};
    use blobstore_core::PublicAccessLevel;

    fn bytes_stream(data: &'static [u8]) -> ByteStream {
        Box::pin(stream::once(async move { Ok(Bytes::from_static(data)) }))
    }

    async fn new_backend() -> (FsBackend, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("meta.sqlite3");
        let backend = FsBackend::open(dir.path(), &db_path).await.unwrap();
        backend.ensure_account("testacct", "a2V5", None).await;
        (backend, dir)
    }

    #[tokio::test]
    async fn create_container_then_get_properties() {
        let (backend, _dir) = new_backend().await;
        backend
            .create_container(
                "testacct",
                "mycontainer",
                PublicAccessLevel::Off,
                HashMap::new(),
            )
            .await
            .unwrap();
        let props = backend
            .get_container_properties("testacct", "mycontainer")
            .await
            .unwrap();
        assert_eq!(props.name, "mycontainer");
        assert_eq!(props.public_access, PublicAccessLevel::Off);
    }

    #[tokio::test]
    async fn create_duplicate_container_fails() {
        let (backend, _dir) = new_backend().await;
        backend
            .create_container("testacct", "dup", PublicAccessLevel::Off, HashMap::new())
            .await
            .unwrap();
        let err = backend
            .create_container("testacct", "dup", PublicAccessLevel::Off, HashMap::new())
            .await
            .unwrap_err();
        assert_eq!(
            err.code,
            blobstore_core::AzureErrorCode::ContainerAlreadyExists
        );
    }

    #[tokio::test]
    async fn put_and_get_blob_round_trips_bytes() {
        let (backend, _dir) = new_backend().await;
        backend
            .create_container("testacct", "c1", PublicAccessLevel::Off, HashMap::new())
            .await
            .unwrap();

        backend
            .put_blob(
                "testacct",
                "c1",
                "hello.txt",
                bytes_stream(b"hello world"),
                Some("text/plain".to_string()),
                None,
                HashMap::new(),
            )
            .await
            .unwrap();

        let payload = backend
            .get_blob("testacct", "c1", "hello.txt", None)
            .await
            .unwrap();
        assert_eq!(payload.total_len, 11);

        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        let mut reader = payload.reader;
        reader.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, b"hello world");
    }

    #[tokio::test]
    async fn range_get_returns_requested_slice() {
        let (backend, _dir) = new_backend().await;
        backend
            .create_container("testacct", "c1", PublicAccessLevel::Off, HashMap::new())
            .await
            .unwrap();
        backend
            .put_blob(
                "testacct",
                "c1",
                "hello.txt",
                bytes_stream(b"hello world"),
                None,
                None,
                HashMap::new(),
            )
            .await
            .unwrap();

        let payload = backend
            .get_blob("testacct", "c1", "hello.txt", Some((6, 10)))
            .await
            .unwrap();

        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        let mut reader = payload.reader;
        reader.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, b"world");
    }

    #[tokio::test]
    async fn block_upload_then_commit_assembles_blob_in_order() {
        let (backend, _dir) = new_backend().await;
        backend
            .create_container("testacct", "c1", PublicAccessLevel::Off, HashMap::new())
            .await
            .unwrap();

        backend
            .put_block("testacct", "c1", "big.bin", "AAAA", bytes_stream(b"first-"))
            .await
            .unwrap();
        backend
            .put_block("testacct", "c1", "big.bin", "BBBB", bytes_stream(b"second"))
            .await
            .unwrap();

        let props = backend
            .put_block_list(
                "testacct",
                "c1",
                "big.bin",
                vec![
                    BlockRef {
                        block_id: "AAAA".to_string(),
                        kind: BlockRefKind::Latest,
                    },
                    BlockRef {
                        block_id: "BBBB".to_string(),
                        kind: BlockRefKind::Latest,
                    },
                ],
                None,
                HashMap::new(),
            )
            .await
            .unwrap();
        assert_eq!(props.content_length, 12);

        let payload = backend
            .get_blob("testacct", "c1", "big.bin", None)
            .await
            .unwrap();
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        let mut reader = payload.reader;
        reader.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, b"first-second");
    }

    #[tokio::test]
    async fn delete_blob_then_get_returns_not_found() {
        let (backend, _dir) = new_backend().await;
        backend
            .create_container("testacct", "c1", PublicAccessLevel::Off, HashMap::new())
            .await
            .unwrap();
        backend
            .put_blob(
                "testacct",
                "c1",
                "x",
                bytes_stream(b"data"),
                None,
                None,
                HashMap::new(),
            )
            .await
            .unwrap();
        backend.delete_blob("testacct", "c1", "x").await.unwrap();
        let err = backend
            .get_blob("testacct", "c1", "x", None)
            .await
            .err()
            .expect("expected BlobNotFound");
        assert_eq!(err.code, blobstore_core::AzureErrorCode::BlobNotFound);
    }

    #[tokio::test]
    async fn list_blobs_respects_prefix() {
        let (backend, _dir) = new_backend().await;
        backend
            .create_container("testacct", "c1", PublicAccessLevel::Off, HashMap::new())
            .await
            .unwrap();
        for name in ["a/1", "a/2", "b/1"] {
            backend
                .put_blob(
                    "testacct",
                    "c1",
                    name,
                    bytes_stream(b"x"),
                    None,
                    None,
                    HashMap::new(),
                )
                .await
                .unwrap();
        }
        let page = backend
            .list_blobs("testacct", "c1", Some("a/"), None, None, 100)
            .await
            .unwrap();
        assert_eq!(page.blobs.len(), 2);
    }
}
