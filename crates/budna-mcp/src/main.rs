mod cli;
mod http_transport;

use anyhow::{Context, Result};
use budna_mcp_client::{ClientConfig, PublicApiClient};
use budna_mcp_core::Transport;
use budna_mcp_server::BudnaMcpServer;
use clap::Parser;
use rmcp::{ServiceExt, transport::stdio};
use tracing_subscriber::{EnvFilter, filter::LevelFilter};

use crate::cli::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    let requested_level = std::env::var("BUDNA_MCP_LOG").ok();
    let filter = logging_filter(requested_level.as_deref());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(std::io::stderr)
        .try_init()
        .map_err(|error| anyhow::anyhow!("failed to initialize stderr logging: {error}"))?;

    let settings = Cli::parse().into_settings()?;
    let client_config =
        ClientConfig::new(settings.api_url())?.with_request_timeout(settings.request_timeout())?;
    let client = PublicApiClient::new(client_config)?;
    let server = BudnaMcpServer::new(client)
        .with_public_urls(settings.public_urls().clone())
        .with_operation_timeout(settings.request_timeout());

    match settings.transport() {
        Transport::Stdio => {
            let service = server.serve(stdio()).await?;
            let cancellation = service.cancellation_token();
            let signal_task = tokio::spawn(async move {
                match tokio::signal::ctrl_c().await {
                    Ok(()) => cancellation.cancel(),
                    Err(error) => {
                        tracing::warn!(error = %error, "Failed to listen for the shutdown signal")
                    }
                }
            });

            service
                .waiting()
                .await
                .context("Budna MCP service task failed")?;
            signal_task.abort();
        }
        Transport::StreamableHttp => {
            http_transport::serve(server, settings.http_server().clone()).await?;
        }
        _ => anyhow::bail!("unsupported Budna MCP transport profile"),
    }

    Ok(())
}

fn logging_filter(requested_level: Option<&str>) -> EnvFilter {
    let level = match requested_level.map(str::trim).map(str::to_ascii_lowercase) {
        Some(value) if value == "off" => LevelFilter::OFF,
        Some(value) if value == "error" => LevelFilter::ERROR,
        Some(value) if value == "warn" => LevelFilter::WARN,
        Some(value) if value == "info" => LevelFilter::INFO,
        Some(value) if value == "debug" => LevelFilter::DEBUG,
        Some(value) if value == "trace" => LevelFilter::TRACE,
        _ => LevelFilter::WARN,
    };

    EnvFilter::new(format!(
        "off,budna_mcp={level},budna_mcp_client={level},budna_mcp_core={level},budna_mcp_server={level}"
    ))
}

#[cfg(test)]
mod logging_tests {
    use super::*;

    #[test]
    fn logging_filter_only_enables_budna_owned_targets() {
        let rendered = logging_filter(Some("trace")).to_string();

        assert!(rendered.contains("budna_mcp=trace"));
        assert!(rendered.contains("budna_mcp_client=trace"));
        assert!(rendered.contains("budna_mcp_core=trace"));
        assert!(rendered.contains("budna_mcp_server=trace"));
        assert!(!rendered.contains("reqwest=trace"));
        assert!(!rendered.contains("hyper=trace"));
    }

    #[test]
    fn invalid_logging_level_fails_closed_to_warn() {
        let rendered = logging_filter(Some("trace,reqwest=trace")).to_string();

        assert!(rendered.contains("budna_mcp=warn"));
        assert!(!rendered.contains("reqwest=trace"));
    }
}
