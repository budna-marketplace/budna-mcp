use thiserror::Error;

use crate::ClientConfigError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestFailureKind {
    Timeout,
    Connect,
    Transport,
}

impl RequestFailureKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Connect => "connect",
            Self::Transport => "transport",
        }
    }
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("could not construct the Budna API endpoint for {operation}")]
    Endpoint {
        operation: &'static str,
        #[source]
        source: ClientConfigError,
    },

    #[error("Budna API request failed during {operation}")]
    Request {
        operation: &'static str,
        kind: RequestFailureKind,
        #[source]
        source: reqwest::Error,
    },

    #[error("Budna API returned HTTP {status} during {operation}")]
    Api {
        operation: &'static str,
        status: u16,
        code: Option<String>,
        title: Option<String>,
        detail: Option<String>,
        retry_after: Option<std::time::Duration>,
    },

    #[error("Budna API returned an invalid JSON response during {operation}")]
    Decode {
        operation: &'static str,
        #[source]
        source: serde_json::Error,
    },

    #[error("Budna API returned an unsuccessful response envelope during {operation}")]
    UnsuccessfulEnvelope { operation: &'static str },

    #[error("Budna API response did not contain data for {operation}")]
    MissingData { operation: &'static str },

    #[error("the requested public Budna resource is unavailable during {operation}")]
    PublicResourceUnavailable {
        operation: &'static str,
        code: &'static str,
        message: &'static str,
    },

    #[error("Budna API response exceeded the {limit_bytes}-byte limit during {operation}")]
    ResponseTooLarge {
        operation: &'static str,
        limit_bytes: usize,
    },

    #[error("Budna API retry handling failed during {operation}")]
    RetryInvariant { operation: &'static str },
}

impl ClientError {
    pub const fn operation(&self) -> &'static str {
        match self {
            Self::Endpoint { operation, .. }
            | Self::Request { operation, .. }
            | Self::Api { operation, .. }
            | Self::Decode { operation, .. }
            | Self::UnsuccessfulEnvelope { operation }
            | Self::MissingData { operation }
            | Self::PublicResourceUnavailable { operation, .. }
            | Self::ResponseTooLarge { operation, .. }
            | Self::RetryInvariant { operation } => operation,
        }
    }

    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Endpoint { .. } => "configuration",
            Self::Request { kind, .. } => kind.as_str(),
            Self::Api { .. } => "api",
            Self::Decode { .. } | Self::UnsuccessfulEnvelope { .. } | Self::MissingData { .. } => {
                "invalid_response"
            }
            Self::PublicResourceUnavailable { .. } => "not_found",
            Self::ResponseTooLarge { .. } => "response_too_large",
            Self::RetryInvariant { .. } => "internal",
        }
    }

    pub const fn status(&self) -> Option<u16> {
        match self {
            Self::Api { status, .. } => Some(*status),
            Self::PublicResourceUnavailable { .. } => Some(404),
            _ => None,
        }
    }

    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Api { code, .. } => code.as_deref(),
            Self::PublicResourceUnavailable { code, .. } => Some(code),
            _ => None,
        }
    }

    pub const fn retryable(&self) -> bool {
        match self {
            Self::Request { .. } => true,
            Self::Api { status, .. } => matches!(status, 429 | 502 | 503 | 504),
            _ => false,
        }
    }

    pub fn public_message(&self) -> String {
        match self {
            Self::Api {
                status,
                title,
                detail,
                ..
            } => {
                let title = title.as_deref().unwrap_or("Budna API error");
                match detail.as_deref() {
                    Some(detail) => format!("{title} (HTTP {status}): {}", truncate(detail, 500)),
                    None => format!("{title} (HTTP {status})"),
                }
            }
            Self::Request { kind, .. } => match kind {
                RequestFailureKind::Timeout => "The Budna API request timed out".to_owned(),
                RequestFailureKind::Connect => "Could not connect to the Budna API".to_owned(),
                RequestFailureKind::Transport => "The Budna API request failed".to_owned(),
            },
            Self::Endpoint { .. } => "The Budna API endpoint is misconfigured".to_owned(),
            Self::PublicResourceUnavailable { message, .. } => {
                format!("{message} (HTTP 404)")
            }
            Self::Decode { .. } | Self::UnsuccessfulEnvelope { .. } | Self::MissingData { .. } => {
                "The Budna API returned an unexpected response".to_owned()
            }
            Self::ResponseTooLarge { .. } => {
                "The Budna API response was too large to return safely".to_owned()
            }
            Self::RetryInvariant { .. } => {
                "The Budna API request could not be retried safely".to_owned()
            }
        }
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}
