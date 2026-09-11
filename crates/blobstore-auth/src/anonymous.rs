use blobstore_core::PublicAccessLevel;

/// Whether an anonymous request may read an individual blob in a container
/// with the given public access level.
pub fn allows_blob_read(level: PublicAccessLevel) -> bool {
    level.allows_blob_read()
}

/// Whether an anonymous request may list blobs / read container properties.
pub fn allows_container_read(level: PublicAccessLevel) -> bool {
    level.allows_container_listing()
}
