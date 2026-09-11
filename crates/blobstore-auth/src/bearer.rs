//! Azure AD / OAuth2 bearer token validation against a configurable OIDC
//! issuer's JWKS, with a background-refreshable cache keyed by `kid`. Since
//! this is a self-hosted server (not real Azure AD), the issuer/audience/JWKS
//! endpoint are all operator-configured rather than hardcoded to Microsoft's.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::AuthError;

#[derive(Debug, Clone)]
pub struct OidcConfig {
    pub issuer: String,
    pub audience: String,
    pub jwks_uri: String,
    pub cache_ttl: Duration,
}

#[derive(Debug, Deserialize)]
struct Jwks {
    keys: Vec<JwkKey>,
}

#[derive(Debug, Deserialize)]
struct JwkKey {
    kid: Option<String>,
    kty: String,
    n: Option<String>,
    e: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Claims {
    pub sub: Option<String>,
    pub iss: Option<String>,
    pub aud: Option<serde_json::Value>,
    pub exp: Option<i64>,
}

struct CacheEntry {
    keys: HashMap<String, DecodingKey>,
    fetched_at: Instant,
}

pub struct JwksCache {
    config: OidcConfig,
    client: reqwest::Client,
    cache: RwLock<Option<CacheEntry>>,
}

impl JwksCache {
    pub fn new(config: OidcConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            client: reqwest::Client::new(),
            cache: RwLock::new(None),
        })
    }

    async fn fetch_keys(&self) -> Result<HashMap<String, DecodingKey>, AuthError> {
        let resp = self
            .client
            .get(&self.config.jwks_uri)
            .send()
            .await
            .map_err(|_| AuthError::JwksUnavailable)?;
        let jwks: Jwks = resp.json().await.map_err(|_| AuthError::JwksUnavailable)?;

        let mut keys = HashMap::new();
        for k in jwks.keys {
            if k.kty != "RSA" {
                continue;
            }
            let (Some(n), Some(e), Some(kid)) = (k.n, k.e, k.kid) else {
                continue;
            };
            if let Ok(dk) = DecodingKey::from_rsa_components(&n, &e) {
                keys.insert(kid, dk);
            }
        }
        Ok(keys)
    }

    async fn get_key(&self, kid: &str) -> Result<DecodingKey, AuthError> {
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.as_ref() {
                if entry.fetched_at.elapsed() < self.config.cache_ttl {
                    if let Some(k) = entry.keys.get(kid) {
                        return Ok(k.clone());
                    }
                }
            }
        }
        // Miss or stale: refresh once and try again.
        let keys = self.fetch_keys().await?;
        let key = keys.get(kid).cloned();
        *self.cache.write().await = Some(CacheEntry {
            keys,
            fetched_at: Instant::now(),
        });
        key.ok_or(AuthError::UnknownSigningKey)
    }

    pub fn issuer(&self) -> &str {
        &self.config.issuer
    }

    pub fn audience(&self) -> &str {
        &self.config.audience
    }
}

/// Validates a bearer JWT: signature (RS256, via the JWKS cache), issuer,
/// audience, expiry. Returns the decoded claims on success.
pub async fn validate_token(cache: &JwksCache, token: &str) -> Result<Claims, AuthError> {
    let header = decode_header(token).map_err(|_| AuthError::MalformedCredential)?;
    let kid = header.kid.ok_or(AuthError::MalformedCredential)?;
    let key = cache.get_key(&kid).await?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[cache.issuer()]);
    validation.set_audience(&[cache.audience()]);

    let data =
        decode::<Claims>(token, &key, &validation).map_err(|_| AuthError::SignatureMismatch)?;
    Ok(data.claims)
}
