use std::time::Duration;

use reqwest::Url;
use thiserror::Error;

pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct ClientConfig {
    api_base_url: Url,
    request_timeout: Duration,
}

impl ClientConfig {
    pub fn new(base_url: impl AsRef<str>) -> Result<Self, ClientConfigError> {
        let api_base_url = normalize_api_base_url(base_url.as_ref())?;

        Ok(Self {
            api_base_url,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        })
    }

    pub fn with_request_timeout(
        mut self,
        request_timeout: Duration,
    ) -> Result<Self, ClientConfigError> {
        if request_timeout.is_zero() {
            return Err(ClientConfigError::ZeroRequestTimeout);
        }

        self.request_timeout = request_timeout;
        Ok(self)
    }

    pub fn base_url(&self) -> &Url {
        &self.api_base_url
    }

    pub fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    pub(crate) fn endpoint(&self, path: &str) -> Result<Url, ClientConfigError> {
        self.api_base_url
            .join(path)
            .map_err(ClientConfigError::InvalidEndpoint)
    }
}

fn normalize_api_base_url(raw: &str) -> Result<Url, ClientConfigError> {
    let trimmed = raw.trim().trim_end_matches('/');

    if trimmed.is_empty() {
        return Err(ClientConfigError::EmptyBaseUrl);
    }

    let mut url = Url::parse(trimmed).map_err(ClientConfigError::InvalidBaseUrl)?;

    if !matches!(url.scheme(), "http" | "https") {
        return Err(ClientConfigError::UnsupportedScheme {
            scheme: url.scheme().to_owned(),
        });
    }

    if url.scheme() == "http" && !is_loopback_host(&url) {
        return Err(ClientConfigError::InsecureRemoteHttp);
    }

    if !url.username().is_empty() || url.password().is_some() {
        return Err(ClientConfigError::CredentialsInBaseUrl);
    }

    if url.query().is_some() || url.fragment().is_some() {
        return Err(ClientConfigError::QueryOrFragmentInBaseUrl);
    }

    let path = url.path().trim_end_matches('/');
    let normalized_path = if path.ends_with("/api/v1") || path == "api/v1" {
        format!("{path}/")
    } else if path.is_empty() {
        "/api/v1/".to_owned()
    } else {
        format!("{path}/api/v1/")
    };
    url.set_path(&normalized_path);

    Ok(url)
}

fn is_loopback_host(url: &Url) -> bool {
    matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"))
}

#[derive(Debug, Error)]
pub enum ClientConfigError {
    #[error("Budna API base URL cannot be empty")]
    EmptyBaseUrl,

    #[error("Budna API base URL must be a valid URL: {0}")]
    InvalidBaseUrl(#[source] url::ParseError),

    #[error("Budna API endpoint could not be constructed: {0}")]
    InvalidEndpoint(#[source] url::ParseError),

    #[error("Budna API base URL must use http or https, got {scheme}")]
    UnsupportedScheme { scheme: String },

    #[error("unencrypted HTTP is only allowed for a loopback Budna API URL")]
    InsecureRemoteHttp,

    #[error("Budna API base URL must not contain embedded credentials")]
    CredentialsInBaseUrl,

    #[error("Budna API base URL must not contain a query string or fragment")]
    QueryOrFragmentInBaseUrl,

    #[error("Budna API request timeout must be greater than zero")]
    ZeroRequestTimeout,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_url_is_normalized_to_api_v1() {
        let config = ClientConfig::new(" https://api.example.test/ ").unwrap_or_else(|error| {
            panic!("base URL should parse: {error}");
        });

        assert_eq!(
            config.base_url().as_str(),
            "https://api.example.test/api/v1/"
        );
    }

    #[test]
    fn existing_api_v1_path_is_preserved_with_trailing_slash() {
        let config = ClientConfig::new("https://api.example.test/api/v1").unwrap_or_else(|error| {
            panic!("base URL should parse: {error}");
        });

        assert_eq!(
            config.base_url().as_str(),
            "https://api.example.test/api/v1/"
        );
        let endpoint = config.endpoint("search/listings").unwrap_or_else(|error| {
            panic!("endpoint should join: {error}");
        });
        assert_eq!(
            endpoint.as_str(),
            "https://api.example.test/api/v1/search/listings"
        );
    }

    #[test]
    fn prefixed_url_gets_api_v1_appended() {
        let config =
            ClientConfig::new("https://api.example.test/preview").unwrap_or_else(|error| {
                panic!("base URL should parse: {error}");
            });

        assert_eq!(
            config.base_url().as_str(),
            "https://api.example.test/preview/api/v1/"
        );
    }

    #[test]
    fn invalid_base_urls_are_rejected() {
        assert!(matches!(
            ClientConfig::new(" "),
            Err(ClientConfigError::EmptyBaseUrl)
        ));
        assert!(matches!(
            ClientConfig::new("ftp://example.test"),
            Err(ClientConfigError::UnsupportedScheme { scheme }) if scheme == "ftp"
        ));
        assert!(matches!(
            ClientConfig::new("http://api.example.test"),
            Err(ClientConfigError::InsecureRemoteHttp)
        ));
        assert!(matches!(
            ClientConfig::new("https://user:secret@example.test"),
            Err(ClientConfigError::CredentialsInBaseUrl)
        ));
        assert!(matches!(
            ClientConfig::new("https://example.test?token=secret"),
            Err(ClientConfigError::QueryOrFragmentInBaseUrl)
        ));
    }

    #[test]
    fn zero_timeout_is_rejected() {
        let config = ClientConfig::new("https://api.example.test").unwrap_or_else(|error| {
            panic!("base URL should parse: {error}");
        });

        assert!(matches!(
            config.with_request_timeout(Duration::ZERO),
            Err(ClientConfigError::ZeroRequestTimeout)
        ));
    }
}
