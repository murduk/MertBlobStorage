//! Service SAS (Shared Access Signature) parsing and verification, signed with
//! an account key (we have no AAD user-delegation path since this is a
//! self-hosted server, not real Azure AD). Pure functions, independently
//! unit-testable.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::AuthError;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Default)]
pub struct SasParams {
    pub sp: String,   // signed permissions, e.g. "rwdl"
    pub st: String,   // signed start (may be empty)
    pub se: String,   // signed expiry (required)
    pub sr: String,   // signed resource: "b" (blob) or "c" (container)
    pub sip: String,  // signed IP range (optional)
    pub spr: String,  // signed protocol: "https" or "https,http"
    pub sv: String,   // signed version
    pub si: String,   // signed identifier (stored access policy - unsupported in v1)
    pub sig: String,  // signature
    pub rscc: String, // response Cache-Control override
    pub rscd: String, // response Content-Disposition override
    pub rsce: String, // response Content-Encoding override
    pub rscl: String, // response Content-Language override
    pub rsct: String, // response Content-Type override
}

/// Parses SAS parameters out of a request's query string. Returns `None` if no
/// `sig` parameter is present (i.e. this isn't a SAS request at all).
pub fn parse_sas_query(query: &[(String, String)]) -> Option<SasParams> {
    let get = |name: &str| -> String {
        query
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    let sig = get("sig");
    if sig.is_empty() {
        return None;
    }
    Some(SasParams {
        sp: get("sp"),
        st: get("st"),
        se: get("se"),
        sr: get("sr"),
        sip: get("sip"),
        spr: get("spr"),
        sv: get("sv"),
        si: get("si"),
        sig,
        rscc: get("rscc"),
        rscd: get("rscd"),
        rsce: get("rsce"),
        rscl: get("rscl"),
        rsct: get("rsct"),
    })
}

/// `canonicalizedResource` for a service SAS: `/blob/{account}/{container}` or
/// `/blob/{account}/{container}/{blob}`.
fn canonicalized_resource(account: &str, container: &str, blob: Option<&str>) -> String {
    match blob {
        Some(b) => format!("/blob/{account}/{container}/{b}"),
        None => format!("/blob/{account}/{container}"),
    }
}

fn string_to_sign(account: &str, container: &str, blob: Option<&str>, p: &SasParams) -> String {
    [
        p.sp.as_str(),
        p.st.as_str(),
        p.se.as_str(),
        canonicalized_resource(account, container, blob).as_str(),
        p.si.as_str(),
        p.sip.as_str(),
        p.spr.as_str(),
        p.sv.as_str(),
        p.sr.as_str(),
        "", // signedSnapshotTime
        "", // signedEncryptionScope
        p.rscc.as_str(),
        p.rscd.as_str(),
        p.rsce.as_str(),
        p.rscl.as_str(),
        p.rsct.as_str(),
    ]
    .join("\n")
}

fn hmac_sign(key_base64: &str, message: &str) -> Result<String, AuthError> {
    let key_bytes = B64
        .decode(key_base64)
        .map_err(|_| AuthError::MalformedCredential)?;
    let mut mac =
        HmacSha256::new_from_slice(&key_bytes).map_err(|_| AuthError::MalformedCredential)?;
    mac.update(message.as_bytes());
    Ok(B64.encode(mac.finalize().into_bytes()))
}

/// Verifies the SAS signature (independent of permission/expiry/resource checks,
/// which callers apply separately via [`check_expiry`] and [`has_permission`]).
pub fn verify_signature(
    account: &str,
    container: &str,
    blob: Option<&str>,
    p: &SasParams,
    key1_base64: &str,
    key2_base64: Option<&str>,
) -> Result<(), AuthError> {
    let sts = string_to_sign(account, container, blob, p);
    if let Ok(sig) = hmac_sign(key1_base64, &sts) {
        if sig == p.sig {
            return Ok(());
        }
    }
    if let Some(key2) = key2_base64 {
        if let Ok(sig) = hmac_sign(key2, &sts) {
            if sig == p.sig {
                return Ok(());
            }
        }
    }
    Err(AuthError::SignatureMismatch)
}

/// Whether `p.se` (signed expiry) is in the future and `p.st` (signed start, if
/// present) is not in the future, per the well-known ISO 8601 SAS date formats.
pub fn check_validity_window(p: &SasParams) -> Result<(), AuthError> {
    let now = time::OffsetDateTime::now_utc();

    if !p.se.is_empty() {
        let expiry = parse_sas_time(&p.se).ok_or(AuthError::MalformedCredential)?;
        if now > expiry {
            return Err(AuthError::Expired);
        }
    }
    if !p.st.is_empty() {
        let start = parse_sas_time(&p.st).ok_or(AuthError::MalformedCredential)?;
        if now < start {
            return Err(AuthError::NotYetValid);
        }
    }
    Ok(())
}

fn parse_sas_time(s: &str) -> Option<time::OffsetDateTime> {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
}

/// Whether the SAS's `sp` string grants `required` (one of `r`,`w`,`d`,`l`,`a`,`c`,`u`,`p`).
pub fn has_permission(p: &SasParams, required: char) -> bool {
    p.sp.contains(required)
}

/// Whether the SAS's `sr` (signed resource) allows the requested scope: a
/// container-scoped SAS (`sr=c`) only authorizes container-level operations
/// (including listing), while a blob-scoped SAS (`sr=b`) only authorizes
/// operations on that specific blob.
pub fn resource_scope_matches(p: &SasParams, requesting_blob: bool) -> bool {
    match p.sr.as_str() {
        "c" => !requesting_blob,
        "b" => requesting_blob,
        _ => false,
    }
}
