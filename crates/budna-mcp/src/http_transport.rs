use std::{
    io,
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result};
use axum::Router;
use budna_mcp_core::HttpServerSettings;
use budna_mcp_server::BudnaMcpServer;
use http::{
    HeaderName, HeaderValue, Method,
    header::{ACCEPT, CONTENT_TYPE},
};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tokio_util::sync::CancellationToken;
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    limit::RequestBodyLimitLayer,
};

const MCP_PATH: &str = "/mcp";
const HTTP_REQUEST_BODY_LIMIT_BYTES: usize = 64 * 1024;
const HTTP_SESSION_IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const LAST_EVENT_ID: HeaderName = HeaderName::from_static("last-event-id");
const MCP_SESSION_ID: HeaderName = HeaderName::from_static("mcp-session-id");
const MCP_PROTOCOL_VERSION: HeaderName = HeaderName::from_static("mcp-protocol-version");

pub async fn serve(server: BudnaMcpServer, settings: HttpServerSettings) -> Result<()> {
    let cancellation = CancellationToken::new();

    let mut session_manager = LocalSessionManager::default();
    session_manager.session_config.keep_alive = Some(HTTP_SESSION_IDLE_TIMEOUT);

    let service = StreamableHttpService::new(
        move || Ok::<_, io::Error>(server.clone()),
        Arc::new(session_manager),
        StreamableHttpServerConfig::default()
            .with_stateful_mode(true)
            .with_allowed_hosts(settings.allowed_hosts.iter().cloned())
            .with_allowed_origins(settings.allowed_origins.iter().cloned())
            .with_cancellation_token(cancellation.child_token()),
    );

    let cors = cors_layer(&settings)?;
    let router = Router::new()
        .nest_service(MCP_PATH, service)
        .layer(RequestBodyLimitLayer::new(HTTP_REQUEST_BODY_LIMIT_BYTES))
        .layer(cors);

    let bind_address = SocketAddr::from((Ipv4Addr::LOCALHOST, settings.port));
    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .with_context(|| format!("failed to bind Budna MCP HTTP server to {bind_address}"))?;
    let local_address = listener
        .local_addr()
        .context("failed to read Budna MCP HTTP listener address")?;

    tracing::info!(
        bind_address = %local_address,
        path = MCP_PATH,
        "Budna MCP Streamable HTTP server started"
    );

    let signal_cancellation = cancellation.clone();
    let signal_task = tokio::spawn(async move {
        match tokio::signal::ctrl_c().await {
            Ok(()) => signal_cancellation.cancel(),
            Err(error) => {
                tracing::warn!(error = %error, "Failed to listen for the shutdown signal")
            }
        }
    });

    let result = axum::serve(listener, router)
        .with_graceful_shutdown(cancellation.clone().cancelled_owned())
        .await;

    cancellation.cancel();
    signal_task.abort();

    result.context("Budna MCP Streamable HTTP server failed")
}

fn cors_layer(settings: &HttpServerSettings) -> Result<CorsLayer> {
    let allowed_origins = settings
        .allowed_origins
        .iter()
        .map(|origin| {
            HeaderValue::from_str(origin)
                .with_context(|| format!("invalid HTTP allowed origin header value: {origin}"))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed_origins))
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers([
            ACCEPT,
            CONTENT_TYPE,
            LAST_EVENT_ID,
            MCP_SESSION_ID,
            MCP_PROTOCOL_VERSION,
        ])
        .expose_headers([MCP_SESSION_ID, MCP_PROTOCOL_VERSION]))
}
