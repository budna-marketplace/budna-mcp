mod cli;
mod http_transport;

use anyhow::{Context, Result};
use budna_mcp_client::{ClientConfig, PublicApiClient};
use budna_mcp_core::{ToolPolicy, Transport};
use budna_mcp_server::BudnaMcpServer;
use clap::Parser;
use rmcp::{ServiceExt, transport::stdio};

use crate::cli::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init()
        .map_err(|error| anyhow::anyhow!("failed to initialize stderr logging: {error}"))?;

    let settings = Cli::parse().into_settings()?;
    let client_config =
        ClientConfig::new(&settings.api_url)?.with_request_timeout(settings.request_timeout)?;
    let client = PublicApiClient::new(client_config)?;
    let server = BudnaMcpServer::new(client, ToolPolicy::public_explore())
        .with_public_urls(settings.public_urls.clone());

    match settings.transport {
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
            http_transport::serve(server, settings.http_server).await?;
        }
    }

    Ok(())
}
