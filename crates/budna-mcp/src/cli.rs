use std::time::Duration;

use budna_mcp_core::{
    ConfigError, DEFAULT_HTTP_PORT, DEFAULT_REQUEST_TIMEOUT_SECS, HttpServerSettings,
    PublicUrlSettings, Settings, Transport,
};
use clap::{Parser, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "budna-mcp",
    version,
    about = "Budna marketplace tools for AI agents."
)]
pub struct Cli {
    #[arg(long, env = "BUDNA_API_URL")]
    api_url: Option<String>,

    #[arg(
        long,
        env = "BUDNA_PUBLIC_LISTING_ORIGIN",
        help = "HTTPS origin used to construct public listing URLs",
        value_name = "HTTPS_ORIGIN"
    )]
    public_listing_origin: Option<String>,

    #[arg(
        long,
        env = "BUDNA_IMAGE_ORIGIN",
        help = "HTTPS origin used to construct public image URLs",
        value_name = "HTTPS_ORIGIN"
    )]
    image_origin: Option<String>,

    #[arg(long, value_enum, default_value_t = CliTransport::Stdio, env = "BUDNA_MCP_TRANSPORT")]
    transport: CliTransport,

    #[arg(
        long,
        default_value_t = DEFAULT_REQUEST_TIMEOUT_SECS,
        env = "BUDNA_MCP_REQUEST_TIMEOUT_SECS",
        value_parser = parse_request_timeout_secs
    )]
    request_timeout_secs: u64,

    #[arg(
        long,
        default_value_t = DEFAULT_HTTP_PORT,
        env = "BUDNA_MCP_HTTP_PORT",
        value_parser = parse_http_port
    )]
    http_port: u16,

    #[arg(long, env = "BUDNA_MCP_HTTP_ALLOWED_HOSTS", value_delimiter = ',')]
    http_allowed_host: Vec<String>,

    #[arg(long, env = "BUDNA_MCP_HTTP_ALLOWED_ORIGINS", value_delimiter = ',')]
    http_allowed_origin: Vec<String>,
}

impl Cli {
    pub fn into_settings(self) -> Result<Settings, ConfigError> {
        let Self {
            api_url,
            public_listing_origin,
            image_origin,
            transport,
            request_timeout_secs,
            http_port,
            http_allowed_host,
            http_allowed_origin,
        } = self;

        let http_server =
            HttpServerSettings::new(http_port, http_allowed_host, http_allowed_origin)?;
        let public_urls = PublicUrlSettings::new(public_listing_origin, image_origin)?;

        Settings::new(
            api_url,
            transport.into(),
            Duration::from_secs(request_timeout_secs),
        )
        .map(|settings| {
            settings
                .with_http_server(http_server)
                .with_public_urls(public_urls)
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CliTransport {
    Stdio,
    StreamableHttp,
}

impl From<CliTransport> for Transport {
    fn from(transport: CliTransport) -> Self {
        match transport {
            CliTransport::Stdio => Self::Stdio,
            CliTransport::StreamableHttp => Self::StreamableHttp,
        }
    }
}

fn parse_request_timeout_secs(value: &str) -> Result<u64, String> {
    let seconds = value
        .parse::<u64>()
        .map_err(|_| "request timeout must be an integer number of seconds".to_owned())?;
    if !(1..=300).contains(&seconds) {
        return Err("request timeout must be between 1 and 300 seconds".to_owned());
    }
    Ok(seconds)
}

fn parse_http_port(value: &str) -> Result<u16, String> {
    let port = value
        .parse::<u16>()
        .map_err(|_| "HTTP port must be an integer between 1 and 65535".to_owned())?;
    if port == 0 {
        return Err("HTTP port must be between 1 and 65535".to_owned());
    }
    Ok(port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_use_public_api() {
        let cli = Cli::try_parse_from(["budna-mcp"]).unwrap_or_else(|error| {
            panic!("default CLI should parse: {error}");
        });
        let settings = cli.into_settings().unwrap_or_else(|error| {
            panic!("default settings should validate: {error}");
        });

        assert_eq!(settings.api_url, budna_mcp_core::DEFAULT_API_URL);
        assert_eq!(settings.request_timeout, Duration::from_secs(30));
        assert_eq!(settings.http_server, HttpServerSettings::default());
        assert_eq!(settings.public_urls, PublicUrlSettings::default());
    }

    #[test]
    fn invalid_timeout_is_rejected_by_clap() {
        let result = Cli::try_parse_from(["budna-mcp", "--request-timeout-secs", "0"]);

        assert!(result.is_err());
    }

    #[test]
    fn streamable_http_options_are_repeatable_and_normalized() {
        let cli = Cli::try_parse_from([
            "budna-mcp",
            "--transport",
            "streamable-http",
            "--http-port",
            "4100",
            "--http-allowed-host",
            "LOCALHOST:4100",
            "--http-allowed-host",
            "127.0.0.1",
            "--http-allowed-origin",
            "http://localhost:8080",
            "--http-allowed-origin",
            "https://app.example.test",
        ])
        .unwrap_or_else(|error| panic!("HTTP CLI options should parse: {error}"));
        let settings = cli
            .into_settings()
            .unwrap_or_else(|error| panic!("HTTP settings should validate: {error}"));

        assert_eq!(settings.transport, Transport::StreamableHttp);
        assert_eq!(settings.http_server.port(), 4100);
        assert_eq!(
            settings.http_server.allowed_hosts(),
            ["localhost:4100", "127.0.0.1"]
        );
        assert_eq!(
            settings.http_server.allowed_origins(),
            ["http://localhost:8080", "https://app.example.test"]
        );
    }

    #[test]
    fn invalid_http_port_is_rejected_by_clap() {
        let result = Cli::try_parse_from(["budna-mcp", "--http-port", "0"]);

        assert!(result.is_err());
    }

    #[test]
    fn public_url_origins_are_configurable_from_the_cli() {
        let cli = Cli::try_parse_from([
            "budna-mcp",
            "--api-url",
            "https://api.example.test/api/v1",
            "--public-listing-origin",
            "https://listings.example.test",
            "--image-origin",
            "https://images.example.test",
        ])
        .unwrap_or_else(|error| panic!("public URL CLI options should parse: {error}"));
        let settings = cli
            .into_settings()
            .unwrap_or_else(|error| panic!("public URL settings should validate: {error}"));

        assert_eq!(settings.api_url, "https://api.example.test/api/v1");
        assert_eq!(
            settings.public_urls.listing_origin(),
            "https://listings.example.test"
        );
        assert_eq!(
            settings.public_urls.image_origin(),
            "https://images.example.test"
        );
    }

    #[test]
    fn insecure_public_url_origins_are_rejected() {
        let cli = Cli::try_parse_from([
            "budna-mcp",
            "--public-listing-origin",
            "http://listings.example.test",
        ])
        .unwrap_or_else(|error| panic!("CLI parsing should defer URL validation: {error}"));

        assert!(cli.into_settings().is_err());
    }
}
