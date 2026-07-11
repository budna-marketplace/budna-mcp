use std::{fmt, time::Duration};

use http::uri::Authority;
use thiserror::Error;
use url::Url;

pub const DEFAULT_API_URL: &str = "https://api.budna.se/api/v1";
pub const DEFAULT_IMAGE_ORIGIN: &str = "https://images.budna.se";
pub const DEFAULT_LISTING_ORIGIN: &str = "https://budna.se";
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;
pub const DEFAULT_HTTP_PORT: u16 = 3001;
pub const DEFAULT_HTTP_ALLOWED_HOSTS: [&str; 2] = ["localhost", "127.0.0.1"];
pub const DEFAULT_HTTP_ALLOWED_ORIGINS: [&str; 2] =
    ["http://localhost:8080", "http://127.0.0.1:8080"];
const MAX_PUBLIC_ORIGIN_LENGTH: usize = 2_048;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Transport {
    Stdio,
    StreamableHttp,
}

impl fmt::Display for Transport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stdio => formatter.write_str("stdio"),
            Self::StreamableHttp => formatter.write_str("streamable-http"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpServerSettings {
    port: u16,
    allowed_hosts: Vec<String>,
    allowed_origins: Vec<String>,
}

impl HttpServerSettings {
    pub fn new(
        port: u16,
        allowed_hosts: Vec<String>,
        allowed_origins: Vec<String>,
    ) -> Result<Self, ConfigError> {
        if port == 0 {
            return Err(ConfigError::ZeroHttpPort);
        }

        let allowed_hosts = normalize_allowed_hosts(allowed_hosts)?;
        let allowed_origins = normalize_allowed_origins(allowed_origins)?;

        Ok(Self {
            port,
            allowed_hosts,
            allowed_origins,
        })
    }

    pub const fn port(&self) -> u16 {
        self.port
    }

    pub fn allowed_hosts(&self) -> &[String] {
        &self.allowed_hosts
    }

    pub fn allowed_origins(&self) -> &[String] {
        &self.allowed_origins
    }
}

impl Default for HttpServerSettings {
    fn default() -> Self {
        Self {
            port: DEFAULT_HTTP_PORT,
            allowed_hosts: DEFAULT_HTTP_ALLOWED_HOSTS.map(str::to_owned).to_vec(),
            allowed_origins: DEFAULT_HTTP_ALLOWED_ORIGINS.map(str::to_owned).to_vec(),
        }
    }
}

/// Public URL origins used in MCP projections and the embedded Marketplace
/// Explorer. These are deliberately configured separately from the API base
/// URL because an API host, public listing host, and image host can differ per
/// deployment environment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicUrlSettings {
    listing_origin: String,
    image_origin: String,
}

impl PublicUrlSettings {
    pub fn new(
        listing_origin: Option<String>,
        image_origin: Option<String>,
    ) -> Result<Self, ConfigError> {
        let listing_origin = normalize_public_origin(
            listing_origin.unwrap_or_else(|| DEFAULT_LISTING_ORIGIN.to_owned()),
            "listing",
        )?;
        let image_origin = normalize_public_origin(
            image_origin.unwrap_or_else(|| DEFAULT_IMAGE_ORIGIN.to_owned()),
            "image",
        )?;

        Ok(Self {
            listing_origin,
            image_origin,
        })
    }

    pub fn listing_origin(&self) -> &str {
        &self.listing_origin
    }

    pub fn image_origin(&self) -> &str {
        &self.image_origin
    }
}

impl Default for PublicUrlSettings {
    fn default() -> Self {
        Self {
            listing_origin: DEFAULT_LISTING_ORIGIN.to_owned(),
            image_origin: DEFAULT_IMAGE_ORIGIN.to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Settings {
    pub api_url: String,
    pub transport: Transport,
    pub request_timeout: Duration,
    pub http_server: HttpServerSettings,
    pub public_urls: PublicUrlSettings,
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
            http_server: HttpServerSettings::default(),
            public_urls: PublicUrlSettings::default(),
        })
    }

    pub fn with_http_server(mut self, http_server: HttpServerSettings) -> Self {
        self.http_server = http_server;
        self
    }

    pub fn with_public_urls(mut self, public_urls: PublicUrlSettings) -> Self {
        self.public_urls = public_urls;
        self
    }
}

fn normalize_api_url(api_url: String) -> Result<String, ConfigError> {
    let trimmed = api_url.trim();

    if trimmed.is_empty() {
        return Err(ConfigError::EmptyApiUrl);
    }

    Ok(trimmed.trim_end_matches('/').to_owned())
}

fn normalize_public_origin(value: String, kind: &'static str) -> Result<String, ConfigError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_PUBLIC_ORIGIN_LENGTH {
        return Err(ConfigError::InvalidPublicOrigin { kind });
    }
    let url = Url::parse(trimmed).map_err(|_| ConfigError::InvalidPublicOrigin { kind })?;

    if url.scheme() != "https"
        || url.host_str().is_none()
        || url.host_str().is_some_and(|host| host.contains('*'))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ConfigError::InvalidPublicOrigin { kind });
    }

    Ok(url.origin().ascii_serialization())
}

fn normalize_allowed_hosts(values: Vec<String>) -> Result<Vec<String>, ConfigError> {
    if values.is_empty() {
        return Ok(DEFAULT_HTTP_ALLOWED_HOSTS.map(str::to_owned).to_vec());
    }

    let mut normalized = Vec::with_capacity(values.len());
    for value in values {
        let trimmed = value.trim();
        let authority =
            Authority::try_from(trimmed).map_err(|_| ConfigError::InvalidHttpAllowedHost)?;

        if authority.host().is_empty()
            || authority.host().contains('*')
            || trimmed.contains(['/', '?', '#', '@'])
        {
            return Err(ConfigError::InvalidHttpAllowedHost);
        }

        let host = authority.as_str().to_ascii_lowercase();
        if !normalized.contains(&host) {
            normalized.push(host);
        }
    }

    Ok(normalized)
}

fn normalize_allowed_origins(values: Vec<String>) -> Result<Vec<String>, ConfigError> {
    if values.is_empty() {
        return Ok(DEFAULT_HTTP_ALLOWED_ORIGINS.map(str::to_owned).to_vec());
    }

    let mut normalized = Vec::with_capacity(values.len());
    for value in values {
        let trimmed = value.trim();
        let url = Url::parse(trimmed).map_err(|_| ConfigError::InvalidHttpAllowedOrigin)?;

        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || url.host_str().is_some_and(|host| host.contains('*'))
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(ConfigError::InvalidHttpAllowedOrigin);
        }

        let origin = url.origin().ascii_serialization();
        if !normalized.contains(&origin) {
            normalized.push(origin);
        }
    }

    Ok(normalized)
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ConfigError {
    #[error("Budna API base URL cannot be empty")]
    EmptyApiUrl,

    #[error("Budna API request timeout must be greater than zero")]
    ZeroRequestTimeout,

    #[error("Budna MCP HTTP port must be greater than zero")]
    ZeroHttpPort,

    #[error("Budna MCP HTTP allowed host is invalid")]
    InvalidHttpAllowedHost,

    #[error("Budna MCP HTTP allowed origin is invalid")]
    InvalidHttpAllowedOrigin,

    #[error("Budna MCP {kind} origin must be a secure URL origin")]
    InvalidPublicOrigin { kind: &'static str },
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIMEOUT: Duration = Duration::from_secs(30);

    #[test]
    fn default_uses_public_api_url_and_stdio_settings() {
        let result = Settings::new(None, Transport::Stdio, TIMEOUT);

        assert_eq!(
            result,
            Ok(Settings {
                api_url: DEFAULT_API_URL.to_owned(),
                transport: Transport::Stdio,
                request_timeout: TIMEOUT,
                http_server: HttpServerSettings::default(),
                public_urls: PublicUrlSettings::default(),
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
                http_server: HttpServerSettings::default(),
                public_urls: PublicUrlSettings::default(),
            })
        );
    }

    #[test]
    fn public_urls_are_normalized_without_inferring_an_environment() {
        let public_urls = PublicUrlSettings::new(
            Some(" HTTPS://LISTINGS.example.test:8443/ ".to_owned()),
            Some("https://images.example.test".to_owned()),
        );

        assert_eq!(
            public_urls,
            Ok(PublicUrlSettings::new(
                Some("https://listings.example.test:8443".to_owned()),
                Some("https://images.example.test".to_owned()),
            )
            .unwrap_or_else(|error| panic!("expected public URLs should validate: {error}")))
        );
    }

    #[test]
    fn settings_can_override_public_urls_without_changing_its_constructor() {
        let public_urls = PublicUrlSettings::new(
            Some("https://listings.example.test".to_owned()),
            Some("https://images.example.test".to_owned()),
        )
        .unwrap_or_else(|error| panic!("public URL settings should validate: {error}"));

        let settings = Settings::new(None, Transport::Stdio, TIMEOUT)
            .unwrap_or_else(|error| panic!("settings should validate: {error}"))
            .with_public_urls(public_urls.clone());

        assert_eq!(settings.public_urls, public_urls);
        assert_eq!(settings.api_url, DEFAULT_API_URL);
    }

    #[test]
    fn custom_http_settings_are_normalized_and_deduplicated() {
        let http_server = HttpServerSettings::new(
            4100,
            vec![
                " LOCALHOST:4100 ".to_owned(),
                "localhost:4100".to_owned(),
                "127.0.0.1".to_owned(),
            ],
            vec![
                " HTTP://LOCALHOST:8080 ".to_owned(),
                "http://localhost:8080".to_owned(),
                "https://app.example.test".to_owned(),
            ],
        );

        let http_server =
            http_server.unwrap_or_else(|error| panic!("HTTP settings should validate: {error}"));
        assert_eq!(http_server.port(), 4100);
        assert_eq!(http_server.allowed_hosts(), ["localhost:4100", "127.0.0.1"]);
        assert_eq!(
            http_server.allowed_origins(),
            ["http://localhost:8080", "https://app.example.test"]
        );
    }

    #[test]
    fn empty_http_lists_restore_secure_defaults() {
        let http_server = HttpServerSettings::new(4100, Vec::new(), Vec::new())
            .unwrap_or_else(|error| panic!("empty HTTP allowlists should use defaults: {error}"));

        assert_eq!(http_server.port(), 4100);
        assert_eq!(http_server.allowed_hosts(), DEFAULT_HTTP_ALLOWED_HOSTS);
        assert_eq!(http_server.allowed_origins(), DEFAULT_HTTP_ALLOWED_ORIGINS);
    }

    #[test]
    fn invalid_http_settings_are_rejected() {
        assert_eq!(
            HttpServerSettings::new(0, Vec::new(), Vec::new()),
            Err(ConfigError::ZeroHttpPort)
        );
        assert_eq!(
            HttpServerSettings::new(3001, vec!["https://localhost".to_owned()], Vec::new()),
            Err(ConfigError::InvalidHttpAllowedHost)
        );
        assert_eq!(
            HttpServerSettings::new(3001, vec!["*.example.test".to_owned()], Vec::new()),
            Err(ConfigError::InvalidHttpAllowedHost)
        );
        assert_eq!(
            HttpServerSettings::new(
                3001,
                Vec::new(),
                vec!["http://localhost:8080/path".to_owned()]
            ),
            Err(ConfigError::InvalidHttpAllowedOrigin)
        );
        assert_eq!(
            HttpServerSettings::new(3001, Vec::new(), vec!["https://*.example.test".to_owned()]),
            Err(ConfigError::InvalidHttpAllowedOrigin)
        );
    }

    #[test]
    fn insecure_or_non_origin_public_urls_are_rejected() {
        for (listing_origin, image_origin) in [
            (Some("http://listings.example.test".to_owned()), None),
            (Some("https://listings.example.test/path".to_owned()), None),
            (Some("https://*.example.test".to_owned()), None),
            (None, Some("https://user@images.example.test".to_owned())),
            (
                None,
                Some("https://images.example.test?query=value".to_owned()),
            ),
        ] {
            assert!(
                PublicUrlSettings::new(listing_origin, image_origin).is_err(),
                "public origins must be canonical HTTPS origins"
            );
        }
    }

    #[test]
    fn invalid_configuration_errors_do_not_echo_sensitive_url_values() {
        let invalid_origin = "https://user:secret@images.example.test/?token=secret";
        let error = match PublicUrlSettings::new(None, Some(invalid_origin.to_owned())) {
            Ok(_) => panic!("credentials and query parameters must be rejected"),
            Err(error) => error,
        };
        assert!(!error.to_string().contains("secret"));

        let invalid_http_origin = "https://user:secret@host.example.test";
        let error =
            match HttpServerSettings::new(3001, Vec::new(), vec![invalid_http_origin.to_owned()]) {
                Ok(_) => panic!("credentials in an Origin allowlist entry must be rejected"),
                Err(error) => error,
            };
        assert!(!error.to_string().contains("secret"));
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
