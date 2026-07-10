use std::{fmt, time::Duration};

use thiserror::Error;

pub const DEFAULT_API_URL: &str = "https://api.budna.se/api/v1";
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Transport {
    Stdio,
}

impl fmt::Display for Transport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stdio => formatter.write_str("stdio"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Settings {
    pub api_url: String,
    pub transport: Transport,
    pub request_timeout: Duration,
}

impl Settings {
    pub fn new(
        api_url: Option<String>,
        transport: Transport,
        request_timeout: Duration,
    ) -> Result<Self, ConfigError> {
        if request_timeout.is_zero() {
            return Err(ConfigError::ZeroRequestTimeout);
        }

        let api_url = api_url.unwrap_or_else(|| DEFAULT_API_URL.to_owned());

        let api_url = normalize_api_url(api_url)?;

        Ok(Self {
            api_url,
            transport,
            request_timeout,
        })
    }
}

fn normalize_api_url(api_url: String) -> Result<String, ConfigError> {
    let trimmed = api_url.trim();

    if trimmed.is_empty() {
        return Err(ConfigError::EmptyApiUrl);
    }

    Ok(trimmed.trim_end_matches('/').to_owned())
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ConfigError {
    #[error("Budna API base URL cannot be empty")]
    EmptyApiUrl,

    #[error("Budna API request timeout must be greater than zero")]
    ZeroRequestTimeout,
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIMEOUT: Duration = Duration::from_secs(30);

    #[test]
    fn default_uses_public_api_url() {
        let result = Settings::new(None, Transport::Stdio, TIMEOUT);

        assert_eq!(
            result,
            Ok(Settings {
                api_url: DEFAULT_API_URL.to_owned(),
                transport: Transport::Stdio,
                request_timeout: TIMEOUT,
            })
        );
    }

    #[test]
    fn explicit_api_url_overrides_default_and_is_normalized() {
        let result = Settings::new(
            Some(" https://api.example.test/ ".to_owned()),
            Transport::Stdio,
            TIMEOUT,
        );

        assert_eq!(
            result,
            Ok(Settings {
                api_url: "https://api.example.test".to_owned(),
                transport: Transport::Stdio,
                request_timeout: TIMEOUT,
            })
        );
    }

    #[test]
    fn empty_api_url_is_rejected() {
        let result = Settings::new(Some("   ".to_owned()), Transport::Stdio, TIMEOUT);

        assert_eq!(result, Err(ConfigError::EmptyApiUrl));
    }

    #[test]
    fn zero_timeout_is_rejected() {
        let result = Settings::new(None, Transport::Stdio, Duration::ZERO);

        assert_eq!(result, Err(ConfigError::ZeroRequestTimeout));
    }
}
