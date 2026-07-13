//! Validated configuration and capability-profile types shared by Budna MCP
//! transports and servers.

pub mod config;
pub mod policy;

pub use config::{
    ConfigError, DEFAULT_API_URL, DEFAULT_HTTP_ALLOWED_HOSTS, DEFAULT_HTTP_ALLOWED_ORIGINS,
    DEFAULT_HTTP_PORT, DEFAULT_IMAGE_ORIGIN, DEFAULT_LISTING_ORIGIN, DEFAULT_REQUEST_TIMEOUT_SECS,
    HttpServerSettings, MAX_REQUEST_TIMEOUT_SECS, PublicUrlSettings, Settings, Transport,
};
pub use policy::{ToolCapability, ToolPolicy};
