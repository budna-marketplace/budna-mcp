use std::time::Duration;

use budna_mcp_core::{ConfigError, DEFAULT_REQUEST_TIMEOUT_SECS, Settings, Transport};
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

    #[arg(long, value_enum, default_value_t = CliTransport::Stdio, env = "BUDNA_MCP_TRANSPORT")]
    transport: CliTransport,

    #[arg(
        long,
        default_value_t = DEFAULT_REQUEST_TIMEOUT_SECS,
        env = "BUDNA_MCP_REQUEST_TIMEOUT_SECS",
        value_parser = parse_request_timeout_secs
    )]
    request_timeout_secs: u64,
}

impl Cli {
    pub fn into_settings(self) -> Result<Settings, ConfigError> {
        Settings::new(
            self.api_url,
            self.transport.into(),
            Duration::from_secs(self.request_timeout_secs),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CliTransport {
    Stdio,
}

impl From<CliTransport> for Transport {
    fn from(transport: CliTransport) -> Self {
        match transport {
            CliTransport::Stdio => Self::Stdio,
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
    }

    #[test]
    fn invalid_timeout_is_rejected_by_clap() {
        let result = Cli::try_parse_from(["budna-mcp", "--request-timeout-secs", "0"]);

        assert!(result.is_err());
    }
}
