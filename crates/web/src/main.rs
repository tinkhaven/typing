//! The server binary.
//!
//! Renders the application, serves practice text, and runs the practice socket.
//! Under `hydrate` this file compiles to nothing — the client is the `cdylib`
//! half of the same crate.

// Leptos view types nest deeply enough that computing the layout of the top-level
// view exceeds rustc's default query depth. `cargo leptos` avoids this by passing
// `--cfg erase_components`, but a plain `cargo build` of this binary needs the
// limit raised so both routes work.
#![recursion_limit = "512"]

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The container health check re-runs this binary rather than shelling out to
    // curl, which keeps the runtime image free of an HTTP client it would need
    // for one line of work.
    if std::env::args().any(|arg| arg == "--health-check") {
        if let Err(error) = health_check() {
            eprintln!("health check failed: {error}");
            std::process::exit(1);
        }
        return Ok(());
    }

    use axum::{
        extract::{Path, State},
        response::{IntoResponse, Json},
        routing::get,
        Router,
    };
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use tower_http::{compression::CompressionLayer, trace::TraceLayer};
    use typing_web::{
        server::{ws, AppState},
        shell, App,
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,typing_web=debug".into()),
        )
        .init();

    let config = get_configuration(None)?;
    let leptos_options = config.leptos_options;
    let address = leptos_options.site_addr;

    let state = AppState::new(leptos_options.clone()).await?;
    let routes = generate_route_list(App);

    /// Serves one language's practice text.
    ///
    /// The client needs the corpus to generate Velocity and Fluidness exercises
    /// locally. It is served as one JSON document per language so the browser can
    /// cache it, rather than embedded in the WASM bundle — all 38 languages would
    /// be 1.7 MB of download for the one a visitor actually uses.
    async fn corpus(
        Path(language): Path<String>,
        State(state): State<AppState>,
    ) -> axum::response::Response {
        match state.corpora.get(&language) {
            Some(corpus) => (
                [(
                    axum::http::header::CACHE_CONTROL,
                    "public, max-age=86400, immutable",
                )],
                Json(corpus.as_ref().clone()),
            )
                .into_response(),
            None => (
                axum::http::StatusCode::NOT_FOUND,
                format!("no practice text for {language}"),
            )
                .into_response(),
        }
    }

    /// Liveness for the load balancer. Deliberately cheap and dependency-free.
    async fn health() -> &'static str {
        "ok"
    }

    let app = Router::new()
        .route("/api/ws", get(ws::handler))
        .route("/api/corpus/{language}", get(corpus))
        .route("/health", get(health))
        .leptos_routes(&state, routes, {
            let leptos_options = leptos_options.clone();
            move || shell(leptos_options.clone())
        })
        .fallback(leptos_axum::file_and_error_handler::<AppState, _>(shell))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&address).await?;
    tracing::info!("listening on http://{address}");
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Asks the running server whether it is healthy, for Docker's `HEALTHCHECK`.
///
/// A bare TCP connect would only prove something is bound to the port, so this
/// makes the request and insists on a 200. Written against `std::net` to keep an
/// HTTP client out of the runtime image.
#[cfg(feature = "ssr")]
fn health_check() -> Result<(), Box<dyn std::error::Error>> {
    use std::{
        io::{Read, Write},
        net::TcpStream,
        time::Duration,
    };

    let address = std::env::var("LEPTOS_SITE_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    // Connect over loopback: the bind address is usually a wildcard, which is
    // not a valid destination.
    let port = address.rsplit(':').next().unwrap_or("8080");
    let target = format!("127.0.0.1:{port}");

    let mut stream = TcpStream::connect(&target)?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    stream.write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    if response.starts_with("HTTP/1.1 200") {
        Ok(())
    } else {
        let status = response.lines().next().unwrap_or("(no response)");
        Err(format!("unexpected response from {target}: {status}").into())
    }
}

/// Waits for Ctrl-C or a container stop, so in-flight sockets close cleanly.
#[cfg(feature = "ssr")]
async fn shutdown_signal() {
    let interrupt = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install Ctrl-C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = interrupt => tracing::info!("interrupted, shutting down"),
        _ = terminate => tracing::info!("terminated, shutting down"),
    }
}

/// The client half of this crate is a `cdylib`; this binary is unused there.
#[cfg(not(feature = "ssr"))]
fn main() {}
