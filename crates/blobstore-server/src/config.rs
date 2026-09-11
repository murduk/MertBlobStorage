use std::path::PathBuf;

use figment::providers::{Env, Format, Serialized, Toml};
use figment::Figment;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub listen_addr: String,
    pub tls_enabled: bool,
    pub tls_cert_path: Option<PathBuf>,
    pub tls_key_path: Option<PathBuf>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:8443".to_string(),
            tls_enabled: false,
            tls_cert_path: None,
            tls_key_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub root_path: PathBuf,
    pub metadata_db_path: PathBuf,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            root_path: PathBuf::from("./data"),
            metadata_db_path: PathBuf::from("./data/metadata.sqlite3"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    pub name: String,
    pub key1: String,
    #[serde(default)]
    pub key2: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OidcConfig {
    pub enabled: bool,
    pub issuer: String,
    pub audience: String,
    pub jwks_uri: String,
    pub jwks_cache_ttl_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub format: String, // "json" | "pretty"
    pub level: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            format: "pretty".to_string(),
            level: "info".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub accounts: Vec<AccountConfig>,
    #[serde(default)]
    pub oidc: OidcConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    /// `x-ms-version` this server reports/accepts.
    #[serde(default = "default_service_version")]
    pub service_version: String,
}

fn default_service_version() -> String {
    "2021-08-06".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            storage: StorageConfig::default(),
            accounts: Vec::new(),
            oidc: OidcConfig::default(),
            logging: LoggingConfig::default(),
            service_version: default_service_version(),
        }
    }
}

/// Loads configuration layered: built-in defaults -> `config_path` (TOML, if it
/// exists) -> environment variables prefixed `MBS_` (double-underscore
/// delimited for nesting, e.g. `MBS_SERVER__LISTEN_ADDR`).
pub fn load(config_path: Option<&PathBuf>) -> anyhow::Result<Config> {
    let mut figment = Figment::from(Serialized::defaults(Config::default()));
    if let Some(path) = config_path {
        if path.exists() {
            figment = figment.merge(Toml::file(path));
        }
    }
    figment = figment.merge(Env::prefixed("MBS_").split("__"));
    Ok(figment.extract()?)
}
