use thiserror::Error;

/// Canonical Azure Blob Storage error codes. Each maps to a fixed HTTP status and
/// a default message; handlers throughout the server produce `AzureError` values
/// built from this enum so every layer (auth, storage, XML, HTTP) reports errors
/// the same way real Azure does, including the `x-ms-error-code` response header
/// that SDKs use to raise typed exceptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AzureErrorCode {
    // Resource existence
    ContainerNotFound,
    ContainerAlreadyExists,
    ContainerBeingDeleted,
    BlobNotFound,
    BlobAlreadyExists,
    ResourceNotFound,

    // Block blob assembly
    InvalidBlockId,
    InvalidBlockList,
    BlockCountExceedsLimit,

    // Auth
    AuthenticationFailed,
    AuthorizationFailure,
    AuthorizationPermissionMismatch,
    AuthorizationResourceTypeMismatch,
    AuthorizationProtocolMismatch,
    AuthorizationSourceIPMismatch,
    InsufficientAccountPermissions,
    InvalidAuthenticationInfo,

    // Request validation
    InvalidHeaderValue,
    InvalidQueryParameterValue,
    InvalidRange,
    MissingRequiredHeader,
    MissingRequiredQueryParameter,
    Md5Mismatch,
    InvalidMd5,
    UnsupportedHeader,
    InvalidXmlDocument,
    InvalidInput,
    OutOfRangeInput,
    RequestUrlFailedToParse,
    InvalidResourceName,
    UnsupportedHttpVerb,

    // Server / not-implemented
    FeatureNotSupported,
    InternalError,
}

impl AzureErrorCode {
    /// Wire string exactly as Azure emits it in `<Code>` and `x-ms-error-code`.
    pub fn code_str(self) -> &'static str {
        match self {
            AzureErrorCode::ContainerNotFound => "ContainerNotFound",
            AzureErrorCode::ContainerAlreadyExists => "ContainerAlreadyExists",
            AzureErrorCode::ContainerBeingDeleted => "ContainerBeingDeleted",
            AzureErrorCode::BlobNotFound => "BlobNotFound",
            AzureErrorCode::BlobAlreadyExists => "BlobAlreadyExists",
            AzureErrorCode::ResourceNotFound => "ResourceNotFound",
            AzureErrorCode::InvalidBlockId => "InvalidBlockId",
            AzureErrorCode::InvalidBlockList => "InvalidBlockList",
            AzureErrorCode::BlockCountExceedsLimit => "BlockCountExceedsLimit",
            AzureErrorCode::AuthenticationFailed => "AuthenticationFailed",
            AzureErrorCode::AuthorizationFailure => "AuthorizationFailure",
            AzureErrorCode::AuthorizationPermissionMismatch => "AuthorizationPermissionMismatch",
            AzureErrorCode::AuthorizationResourceTypeMismatch => {
                "AuthorizationResourceTypeMismatch"
            }
            AzureErrorCode::AuthorizationProtocolMismatch => "AuthorizationProtocolMismatch",
            AzureErrorCode::AuthorizationSourceIPMismatch => "AuthorizationSourceIPMismatch",
            AzureErrorCode::InsufficientAccountPermissions => "InsufficientAccountPermissions",
            AzureErrorCode::InvalidAuthenticationInfo => "InvalidAuthenticationInfo",
            AzureErrorCode::InvalidHeaderValue => "InvalidHeaderValue",
            AzureErrorCode::InvalidQueryParameterValue => "InvalidQueryParameterValue",
            AzureErrorCode::InvalidRange => "InvalidRange",
            AzureErrorCode::MissingRequiredHeader => "MissingRequiredHeader",
            AzureErrorCode::MissingRequiredQueryParameter => "MissingRequiredQueryParameter",
            AzureErrorCode::Md5Mismatch => "Md5Mismatch",
            AzureErrorCode::InvalidMd5 => "InvalidMd5",
            AzureErrorCode::UnsupportedHeader => "UnsupportedHeader",
            AzureErrorCode::InvalidXmlDocument => "InvalidXmlDocument",
            AzureErrorCode::InvalidInput => "InvalidInput",
            AzureErrorCode::OutOfRangeInput => "OutOfRangeInput",
            AzureErrorCode::RequestUrlFailedToParse => "RequestUrlFailedToParse",
            AzureErrorCode::InvalidResourceName => "InvalidResourceName",
            AzureErrorCode::UnsupportedHttpVerb => "UnsupportedHttpVerb",
            AzureErrorCode::FeatureNotSupported => "FeatureNotSupported",
            AzureErrorCode::InternalError => "InternalError",
        }
    }

    /// HTTP status code Azure returns for this error.
    pub fn status_code(self) -> u16 {
        match self {
            AzureErrorCode::ContainerNotFound
            | AzureErrorCode::BlobNotFound
            | AzureErrorCode::ResourceNotFound => 404,

            AzureErrorCode::ContainerAlreadyExists | AzureErrorCode::BlobAlreadyExists => 409,
            AzureErrorCode::ContainerBeingDeleted => 409,

            AzureErrorCode::InvalidBlockId
            | AzureErrorCode::InvalidBlockList
            | AzureErrorCode::BlockCountExceedsLimit
            | AzureErrorCode::InvalidHeaderValue
            | AzureErrorCode::InvalidQueryParameterValue
            | AzureErrorCode::MissingRequiredHeader
            | AzureErrorCode::MissingRequiredQueryParameter
            | AzureErrorCode::Md5Mismatch
            | AzureErrorCode::InvalidMd5
            | AzureErrorCode::UnsupportedHeader
            | AzureErrorCode::InvalidXmlDocument
            | AzureErrorCode::InvalidInput
            | AzureErrorCode::OutOfRangeInput
            | AzureErrorCode::RequestUrlFailedToParse
            | AzureErrorCode::InvalidResourceName => 400,

            AzureErrorCode::InvalidRange => 416,

            AzureErrorCode::AuthenticationFailed | AzureErrorCode::InvalidAuthenticationInfo => 401,

            AzureErrorCode::AuthorizationFailure
            | AzureErrorCode::AuthorizationPermissionMismatch
            | AzureErrorCode::AuthorizationResourceTypeMismatch
            | AzureErrorCode::AuthorizationProtocolMismatch
            | AzureErrorCode::AuthorizationSourceIPMismatch
            | AzureErrorCode::InsufficientAccountPermissions => 403,

            AzureErrorCode::UnsupportedHttpVerb => 405,
            AzureErrorCode::FeatureNotSupported => 501,
            AzureErrorCode::InternalError => 500,
        }
    }

    /// Default human-readable message, matching Azure's documented wording closely
    /// enough for client display purposes. Callers may override with `AzureError::with_message`.
    pub fn default_message(self) -> &'static str {
        match self {
            AzureErrorCode::ContainerNotFound => "The specified container does not exist.",
            AzureErrorCode::ContainerAlreadyExists => {
                "The specified container already exists."
            }
            AzureErrorCode::ContainerBeingDeleted => {
                "The specified container is being deleted."
            }
            AzureErrorCode::BlobNotFound => "The specified blob does not exist.",
            AzureErrorCode::BlobAlreadyExists => "The specified blob already exists.",
            AzureErrorCode::ResourceNotFound => "The specified resource does not exist.",
            AzureErrorCode::InvalidBlockId => "The specified block ID is invalid.",
            AzureErrorCode::InvalidBlockList => "The specified block list is invalid.",
            AzureErrorCode::BlockCountExceedsLimit => {
                "The committed block count cannot exceed the maximum limit."
            }
            AzureErrorCode::AuthenticationFailed => "Server failed to authenticate the request.",
            AzureErrorCode::AuthorizationFailure => {
                "This request is not authorized to perform this operation."
            }
            AzureErrorCode::AuthorizationPermissionMismatch => {
                "This request is not authorized to perform this operation using this permission."
            }
            AzureErrorCode::AuthorizationResourceTypeMismatch => {
                "This request is not authorized to perform this operation using this resource type."
            }
            AzureErrorCode::AuthorizationProtocolMismatch => {
                "The request is not authorized because the protocol does not match the one specified in the signature."
            }
            AzureErrorCode::AuthorizationSourceIPMismatch => {
                "The request is not authorized from the source IP address."
            }
            AzureErrorCode::InsufficientAccountPermissions => {
                "The account being accessed does not have sufficient permissions to execute this operation."
            }
            AzureErrorCode::InvalidAuthenticationInfo => {
                "The authentication information was not provided in the correct format."
            }
            AzureErrorCode::InvalidHeaderValue => "The value provided for one of the HTTP headers was not in the correct format.",
            AzureErrorCode::InvalidQueryParameterValue => {
                "An invalid value was specified for one of the query parameters."
            }
            AzureErrorCode::InvalidRange => "The range specified is invalid for the current size of the resource.",
            AzureErrorCode::MissingRequiredHeader => "A required HTTP header was not specified.",
            AzureErrorCode::MissingRequiredQueryParameter => {
                "A required query parameter was not specified."
            }
            AzureErrorCode::Md5Mismatch => "The MD5 value specified in the request did not match the MD5 value calculated by the server.",
            AzureErrorCode::InvalidMd5 => "The MD5 value specified in the request is invalid.",
            AzureErrorCode::UnsupportedHeader => "One of the HTTP headers specified in the request is not supported.",
            AzureErrorCode::InvalidXmlDocument => "The specified XML is not syntactically valid.",
            AzureErrorCode::InvalidInput => "One of the request inputs is not valid.",
            AzureErrorCode::OutOfRangeInput => "One of the request inputs is out of range.",
            AzureErrorCode::RequestUrlFailedToParse => "The url in the request could not be parsed.",
            AzureErrorCode::InvalidResourceName => {
                "The specified resource name contains invalid characters."
            }
            AzureErrorCode::UnsupportedHttpVerb => "The resource doesn't support the specified HTTP verb.",
            AzureErrorCode::FeatureNotSupported => "This feature is not supported by this server.",
            AzureErrorCode::InternalError => "The server encountered an internal error. Please try again.",
        }
    }
}

/// A fully-formed Azure-shaped error: a code (which fixes the HTTP status and
/// `x-ms-error-code` header) plus a message. Every fallible operation in every
/// crate should ultimately produce one of these so `blobstore-api` can render it
/// as `<Error><Code>...</Code><Message>...</Message></Error>` uniformly.
#[derive(Debug, Error, Clone)]
#[error("{code}: {message}")]
pub struct AzureError {
    pub code: AzureErrorCode,
    pub message: String,
}

impl std::fmt::Display for AzureErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code_str())
    }
}

impl AzureError {
    pub fn new(code: AzureErrorCode) -> Self {
        Self {
            code,
            message: code.default_message().to_string(),
        }
    }

    pub fn with_message(code: AzureErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn status_code(&self) -> u16 {
        self.code.status_code()
    }
}

impl From<AzureErrorCode> for AzureError {
    fn from(code: AzureErrorCode) -> Self {
        AzureError::new(code)
    }
}
