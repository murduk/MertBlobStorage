mod config;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use blobstore_api::AppState;
use blobstore_auth::bearer::{JwksCache, OidcConfig as AuthOidcConfig};
use blobstore_storage::FsBackend;
use clap::Parser;
use rand::RngCore;

#[derive(Parser, Debug)]
#[command(
    name = "mertblobstorage",
    about = "Self-hosted Azure Blob Storage-compatible server"
)]
struct Cli {
    /// Path to a TOML config file (optional; env vars and defaults apply regardless).
    #[arg(long, default_value = "config/default.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let cfg = config::load(Some(&cli.config))?;

    init_tracing(&cfg.logging);

    tracing::info!(root = %cfg.storage.root_path.display(), "starting MertBlobStorage");

    let backend =
        Arc::new(FsBackend::open(&cfg.storage.root_path, &cfg.storage.metadata_db_path).await?);

    if cfg.accounts.is_empty() {
        let mut key_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key_bytes);
        let key1 = B64.encode(key_bytes);
        backend
            .ensure_account("devstoreaccount1", &key1, None)
            .await;
        tracing::warn!(
            account = "devstoreaccount1",
            key1 = %key1,
            "no accounts configured; generated a development account. Add it to your config file to persist it across restarts."
        );
    } else {
        for acc in &cfg.accounts {
            backend
                .ensure_account(&acc.name, &acc.key1, acc.key2.as_deref())
                .await;
        }
    }

    let oidc: Option<Arc<JwksCache>> = if cfg.oidc.enabled {
        Some(JwksCache::new(AuthOidcConfig {
            issuer: cfg.oidc.issuer.clone(),
            audience: cfg.oidc.audience.clone(),
            jwks_uri: cfg.oidc.jwks_uri.clone(),
            cache_ttl: Duration::from_secs(cfg.oidc.jwks_cache_ttl_seconds.max(60)),
        }))
    } else {
        None
    };

    let scheme = if cfg.server.tls_enabled {
        "https"
    } else {
        "http"
    };
    let service_endpoint = format!("{scheme}://{}/", cfg.server.listen_addr);

    let state = AppState {
        backend,
        oidc,
        service_version: cfg.service_version.clone(),
        service_endpoint,
    };

    let app = blobstore_api::build_router(state);
    let addr: SocketAddr = cfg.server.listen_addr.parse()?;

    if cfg.server.tls_enabled {
        let cert = cfg.server.tls_cert_path.clone().ok_or_else(|| {
            anyhow::anyhow!("server.tls_enabled is true but tls_cert_path is not set")
        })?;
        let key = cfg.server.tls_key_path.clone().ok_or_else(|| {
            anyhow::anyhow!("server.tls_enabled is true but tls_key_path is not set")
        })?;
        let tls_config = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert, key).await?;
        tracing::info!(%addr, "listening (TLS)");
        axum_server::bind_rustls(addr, tls_config)
            .serve(app.into_make_service())
            .await?;
    } else {
        tracing::info!(%addr, "listening (plaintext — TLS is disabled; do not expose this directly to untrusted networks)");
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app.into_make_service())
            .with_graceful_shutdown(shutdown_signal())
            .await?;
    }

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received, draining in-flight requests");
}

fn init_tracing(cfg: &config::LoggingConfig) {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_new(&cfg.level).unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber = tracing_subscriber::fmt().with_env_filter(filter);
    if cfg.format == "json" {
        subscriber.json().init();
    } else {
        subscriber.init();
    }
}
