use serde::{Deserialize, Serialize};

/// Container-level public access, mirroring Azure's `x-ms-blob-public-access` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PublicAccessLevel {
    /// No anonymous access. Requests must be authenticated.
    #[default]
    Off,
    /// Anonymous read access to individual blobs, but not to the container listing.
    Blob,
    /// Anonymous read access to blobs and to the container's blob list.
    Container,
}

impl PublicAccessLevel {
    pub fn as_header_value(self) -> Option<&'static str> {
        match self {
            PublicAccessLevel::Off => None,
            PublicAccessLevel::Blob => Some("blob"),
            PublicAccessLevel::Container => Some("container"),
        }
    }

    pub fn from_header_value(v: Option<&str>) -> Self {
        match v {
            Some("blob") => PublicAccessLevel::Blob,
            Some("container") => PublicAccessLevel::Container,
            _ => PublicAccessLevel::Off,
        }
    }

    pub fn as_db_str(self) -> &'static str {
        match self {
            PublicAccessLevel::Off => "off",
            PublicAccessLevel::Blob => "blob",
            PublicAccessLevel::Container => "container",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "blob" => PublicAccessLevel::Blob,
            "container" => PublicAccessLevel::Container,
            _ => PublicAccessLevel::Off,
        }
    }

    /// Whether anonymous read of an individual blob is allowed.
    pub fn allows_blob_read(self) -> bool {
        matches!(self, PublicAccessLevel::Blob | PublicAccessLevel::Container)
    }

    /// Whether anonymous container-level listing is allowed.
    pub fn allows_container_listing(self) -> bool {
        matches!(self, PublicAccessLevel::Container)
    }
}

/// Blob type. Only `BlockBlob` is implemented in v1; the variants exist so the
/// schema and wire format don't need breaking changes when Append/Page blobs land.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlobType {
    BlockBlob,
    AppendBlob,
    PageBlob,
}

impl BlobType {
    pub fn as_str(self) -> &'static str {
        match self {
            BlobType::BlockBlob => "BlockBlob",
            BlobType::AppendBlob => "AppendBlob",
            BlobType::PageBlob => "PageBlob",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "BlockBlob" => Some(BlobType::BlockBlob),
            "AppendBlob" => Some(BlobType::AppendBlob),
            "PageBlob" => Some(BlobType::PageBlob),
            _ => None,
        }
    }
}
