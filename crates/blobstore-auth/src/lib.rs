//! Authentication/authorization primitives for the four schemes Azure Blob
//! Storage supports: Shared Key, SAS, anonymous public access, and Azure AD /
//! OAuth2 bearer tokens. Each scheme's crypto/validation logic is a pure,
//! independently-unit-testable function; `blobstore-api` wires them together
//! into one `AuthContext` axum extractor so handlers stay auth-scheme-agnostic.

pub mod anonymous;
pub mod bearer;
pub mod sas;
pub mod shared_key;
#[cfg(test)]
mod shared_key_tests;

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AuthError {
    #[error("no credentials were provided")]
    NoCredentials,
    #[error("the credential was malformed")]
    MalformedCredential,
    #[error("the computed signature did not match")]
    SignatureMismatch,
    #[error("the signing key referenced by the token is unknown")]
    UnknownSigningKey,
    #[error("the JWKS endpoint could not be reached")]
    JwksUnavailable,
    #[error("the credential has expired")]
    Expired,
    #[error("the credential is not yet valid")]
    NotYetValid,
    #[error("the credential does not grant the requested permission")]
    PermissionDenied,
    #[error("the credential's resource scope does not match the request")]
    ResourceScopeMismatch,
    #[error("the request's date header is outside the allowed clock skew")]
    ClockSkewTooLarge,
    #[error("unknown account")]
    UnknownAccount,
}

/// How the caller authenticated, attached to the request for logging/audit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthMethod {
    SharedKey,
    Sas,
    Bearer { subject: Option<String> },
    Anonymous,
}

/// The outcome of authenticating+authorizing a request, regardless of which of
/// the four schemes produced it. Handlers only ever look at this, never at the
/// scheme that produced it.
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub account: String,
    pub method: AuthMethod,
}

/// Permission characters as used in a SAS's `sp` field and, conceptually, for
/// every operation this server exposes: (r)ead, (w)rite, (d)elete, (l)ist,
/// (a)dd, (c)reate, (u)pdate, (p)rocess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    Read,
    Write,
    Delete,
    List,
    Add,
    Create,
}

impl Permission {
    pub fn sas_char(self) -> char {
        match self {
            Permission::Read => 'r',
            Permission::Write => 'w',
            Permission::Delete => 'd',
            Permission::List => 'l',
            Permission::Add => 'a',
            Permission::Create => 'c',
        }
    }
}
