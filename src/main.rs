//! ostore daemon entry point: config load, state init, HTTP serving with
//! graceful shutdown on SIGTERM/SIGINT.

use ostore::AppState;
use ostore::build_router;
use ostore::config::Config;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cfg = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("ostore: config error: {e}");
            std::process::exit(2);
        }
    };
    let state = match ostore::s3::S3Auth::from_env() {
        Ok(s3) => match AppState::with_s3(cfg.root.clone(), cfg.token.clone(), s3) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("ostore: cannot initialize root {}: {e}", cfg.root.display());
                std::process::exit(2);
            }
        },
        Err(e) => {
            eprintln!("ostore: config error: {e}");
            std::process::exit(2);
        }
    };
    if state.s3.is_some() {
        tracing::info!("S3 read surface enabled");
    }
    let swept = tokio::task::spawn_blocking({
        let root = cfg.root.clone();
        move || {
            let thumbs = ostore::routes::thumb::sweep_thumbs(&root);
            if thumbs > 0 {
                tracing::info!(removed = thumbs, "swept stale thumbnails");
            }
            ostore::sweep_stale_tmp(&root)
        }
    })
    .await
    .unwrap_or(0);
    state.metrics.tmp_swept.store(swept as u64, std::sync::atomic::Ordering::Relaxed);
    let shutdown = state.shutdown.clone();
    let app = build_router(state);
    let listener = match tokio::net::TcpListener::bind((cfg.host.as_str(), cfg.port)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("ostore: cannot bind {}:{}: {e}", cfg.host, cfg.port);
            std::process::exit(2);
        }
    };
    tracing::info!(addr = %format!("{}:{}", cfg.host, cfg.port), root = %cfg.root.display(), "ostore listening");
    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            // Ends open SSE streams; otherwise graceful shutdown waits on them forever.
            shutdown.cancel();
        })
        .await
    {
        eprintln!("ostore: server error: {e}");
        std::process::exit(1);
    }
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {
                    _ = ctrl_c => {},
                    _ = term.recv() => {},
                }
            }
            Err(_) => {
                let _ = ctrl_c.await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = ctrl_c.await;
    }
    tracing::info!("shutdown signal received");
}
