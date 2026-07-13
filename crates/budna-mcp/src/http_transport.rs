use std::{
    io,
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result};
use axum::{
    Router,
    extract::{Request, State},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use budna_mcp_core::HttpServerSettings;
use budna_mcp_server::BudnaMcpServer;
use http::{
    HeaderMap, HeaderName, HeaderValue, Method,
    header::{ACCEPT, CONTENT_TYPE, HOST, ORIGIN},
    uri::Authority,
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

#[derive(Clone)]
struct HttpAccessPolicy {
    allowed_hosts: Arc<[String]>,
    allowed_origins: Arc<[String]>,
}

pub async fn serve(server: BudnaMcpServer, settings: HttpServerSettings) -> Result<()> {
    let cancellation = CancellationToken::new();

    let mut session_manager = LocalSessionManager::default();
    session_manager.session_config.keep_alive = Some(HTTP_SESSION_IDLE_TIMEOUT);

    let service = StreamableHttpService::new(
        move || Ok::<_, io::Error>(server.clone()),
        Arc::new(session_manager),
        StreamableHttpServerConfig::default()
            .with_stateful_mode(true)
            .with_allowed_hosts(settings.allowed_hosts().iter().cloned())
            .with_allowed_origins(settings.allowed_origins().iter().cloned())
            .with_cancellation_token(cancellation.child_token()),
    );

    let cors = cors_layer(&settings)?;
    let access_policy = HttpAccessPolicy {
        allowed_hosts: Arc::from(settings.allowed_hosts().to_vec()),
        allowed_origins: Arc::from(settings.allowed_origins().to_vec()),
    };
    let router = Router::new()
        .nest_service(MCP_PATH, service)
        .layer(RequestBodyLimitLayer::new(HTTP_REQUEST_BODY_LIMIT_BYTES))
        .layer(cors)
        .layer(middleware::from_fn_with_state(
            access_policy,
            validate_http_access,
        ));

    let bind_address = SocketAddr::from((Ipv4Addr::LOCALHOST, settings.port()));
    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .with_context(|| format!("failed to bind Budna MCP HTTP server to {bind_address}"))?;
    let local_address = listener
        .local_addr()
        .context("failed to read Budna MCP HTTP listener address")?;

    if settings.port() == 0 {
        tracing::warn!(
            bind_address = %local_address,
            path = MCP_PATH,
            "Budna MCP assigned an ephemeral Streamable HTTP port"
        );
    } else {
        tracing::info!(
            bind_address = %local_address,
            path = MCP_PATH,
            "Budna MCP Streamable HTTP server started"
        );
    }

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
        .allowed_origins()
        .iter()
        .map(|origin| {
            HeaderValue::from_str(origin)
                .context("invalid configured HTTP allowed origin header value")
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

async fn validate_http_access(
    State(policy): State<HttpAccessPolicy>,
    request: Request,
    next: Next,
) -> Response {
    if !has_allowed_host(request.headers(), &policy.allowed_hosts)
        || !has_allowed_origin(request.headers(), &policy.allowed_origins)
    {
        return http::StatusCode::FORBIDDEN.into_response();
    }

    next.run(request).await
}

fn has_allowed_host(headers: &HeaderMap, allowed_hosts: &[String]) -> bool {
    let Some(host) = single_header_value(headers, HOST) else {
        return false;
    };
    let Ok(host) = host.to_str() else {
        return false;
    };
    let Ok(host) = Authority::try_from(host) else {
        return false;
    };
    let normalized_host = host.as_str().to_ascii_lowercase();

    allowed_hosts.iter().any(|allowed| {
        if allowed == &normalized_host {
            return true;
        }

        let Ok(allowed) = Authority::try_from(allowed.as_str()) else {
            return false;
        };
        allowed.port().is_none() && allowed.host().eq_ignore_ascii_case(host.host())
    })
}

fn has_allowed_origin(headers: &HeaderMap, allowed_origins: &[String]) -> bool {
    let Some(origin) = single_header_value(headers, ORIGIN) else {
        return headers.get_all(ORIGIN).iter().next().is_none();
    };

    allowed_origins
        .iter()
        .any(|allowed| origin.as_bytes() == allowed.as_bytes())
}

fn single_header_value(headers: &HeaderMap, name: HeaderName) -> Option<&HeaderValue> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?;
    values.next().is_none().then_some(value)
}
