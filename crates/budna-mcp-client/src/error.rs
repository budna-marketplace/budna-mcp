use thiserror::Error;

use crate::ClientConfigError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
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
#[non_exhaustive]
pub enum ClientError {
    #[error("invalid input for {operation}: {message}")]
    InvalidInput {
        operation: &'static str,
        code: &'static str,
        message: &'static str,
    },

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

    #[error("Budna API response violated the public contract during {operation}")]
    InvalidResponse { operation: &'static str },

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

    #[error("Budna API request remained unavailable after {attempts} attempts during {operation}")]
    RetryExhausted {
        operation: &'static str,
        attempts: u32,
        #[source]
        last_error: Box<ClientError>,
    },
}

impl ClientError {
    pub const fn operation(&self) -> &'static str {
        match self {
            Self::InvalidInput { operation, .. }
            | Self::Endpoint { operation, .. }
            | Self::Request { operation, .. }
            | Self::Api { operation, .. }
            | Self::Decode { operation, .. }
            | Self::UnsuccessfulEnvelope { operation }
            | Self::MissingData { operation }
            | Self::InvalidResponse { operation }
            | Self::PublicResourceUnavailable { operation, .. }
            | Self::ResponseTooLarge { operation, .. }
            | Self::RetryInvariant { operation }
            | Self::RetryExhausted { operation, .. } => operation,
        }
    }

    pub const fn kind(&self) -> &'static str {
        match self {
            Self::InvalidInput { .. } => "invalid_input",
            Self::Endpoint { .. } => "configuration",
            Self::Request { kind, .. } => kind.as_str(),
            Self::Api { .. } => "api",
            Self::Decode { .. }
            | Self::UnsuccessfulEnvelope { .. }
            | Self::MissingData { .. }
            | Self::InvalidResponse { .. } => "invalid_response",
            Self::PublicResourceUnavailable { .. } => "not_found",
            Self::ResponseTooLarge { .. } => "response_too_large",
            Self::RetryInvariant { .. } => "internal",
            Self::RetryExhausted { .. } => "retry_exhausted",
        }
    }

    pub fn status(&self) -> Option<u16> {
        match self {
            Self::InvalidInput { .. } => Some(400),
            Self::Api { status, .. } => Some(*status),
            Self::PublicResourceUnavailable { .. } => Some(404),
            Self::RetryExhausted { last_error, .. } => last_error.status(),
            _ => None,
        }
    }

    pub fn code(&self) -> Option<&str> {
        match self {
            Self::InvalidInput { code, .. } => Some(code),
            Self::Api { code, .. } => code.as_deref(),
            Self::PublicResourceUnavailable { code, .. } => Some(code),
            Self::RetryExhausted { last_error, .. } => last_error.code(),
            _ => None,
        }
    }

    /// A fixed code suitable for an MCP response. Never expose a backend-provided
    /// problem code to the model, even when it was syntactically sanitized.
    pub const fn public_code(&self) -> Option<&'static str> {
        match self {
            Self::InvalidInput { code, .. } => Some(code),
            Self::Endpoint { .. } => Some("BUDNA_API_CONFIGURATION_ERROR"),
            Self::Request { kind, .. } => Some(match kind {
                RequestFailureKind::Timeout => "BUDNA_API_TIMEOUT",
                RequestFailureKind::Connect => "BUDNA_API_CONNECT_ERROR",
                RequestFailureKind::Transport => "BUDNA_API_TRANSPORT_ERROR",
            }),
            Self::Api { status, .. } => Some(public_api_error_code(*status)),
            Self::PublicResourceUnavailable { code, .. } => Some(code),
            Self::Decode { .. }
            | Self::UnsuccessfulEnvelope { .. }
            | Self::MissingData { .. }
            | Self::InvalidResponse { .. } => Some("BUDNA_API_INVALID_RESPONSE"),
            Self::ResponseTooLarge { .. } => Some("BUDNA_API_RESPONSE_TOO_LARGE"),
            Self::RetryInvariant { .. } => Some("BUDNA_API_RETRY_ERROR"),
            Self::RetryExhausted { .. } => Some("BUDNA_API_RETRY_EXHAUSTED"),
        }
    }

    pub fn retryable(&self) -> bool {
        match self {
            Self::Request { .. } => true,
            Self::Api { status, code, .. } => {
                matches!(status, 429 | 502 | 503 | 504)
                    || (matches!(status, 408 | 500..=599)
                        && code.as_deref().is_some_and(is_retryable_problem_code))
            }
            Self::RetryExhausted { .. } => true,
            _ => false,
        }
    }

    /// A bounded server-provided delay that callers may use before trying again.
    pub fn retry_after(&self) -> Option<std::time::Duration> {
        match self {
            Self::Api { retry_after, .. } => *retry_after,
            Self::RetryExhausted { last_error, .. } => last_error.retry_after(),
            _ => None,
        }
    }

    pub fn public_message(&self) -> String {
        match self {
            Self::InvalidInput { message, .. } => message.to_string(),
            Self::Api { status, .. } => {
                format!("{} (HTTP {status})", public_api_error_message(*status))
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
            Self::Decode { .. }
            | Self::UnsuccessfulEnvelope { .. }
            | Self::MissingData { .. }
            | Self::InvalidResponse { .. } => {
                "The Budna API returned an unexpected response".to_owned()
            }
            Self::ResponseTooLarge { .. } => {
                "The Budna API response was too large to return safely".to_owned()
            }
            Self::RetryInvariant { .. } => {
                "The Budna API request could not be retried safely".to_owned()
            }
            Self::RetryExhausted { .. } => {
                "The Budna API remained unavailable after bounded retries".to_owned()
            }
        }
    }
}

const fn public_api_error_code(status: u16) -> &'static str {
    match status {
        400 => "INVALID_REQUEST",
        401 => "API_UNAUTHORIZED",
        403 => "API_FORBIDDEN",
        404 => "API_NOT_FOUND",
        408 => "API_TIMEOUT",
        429 => "API_RATE_LIMITED",
        500..=599 => "BUDNA_API_UNAVAILABLE",
        _ => "BUDNA_API_ERROR",
    }
}

const fn public_api_error_message(status: u16) -> &'static str {
    match status {
        400 => "Budna API rejected the request",
        401 => "Budna API request is not authorized",
        403 => "Budna API request is not permitted",
        404 => "Budna API resource was not found",
        408 => "Budna API request timed out",
        429 => "Budna API request was rate limited",
        500..=599 => "Budna API unavailable",
        _ => "Budna API request failed",
    }
}

fn is_retryable_problem_code(code: &str) -> bool {
    matches!(
        code,
        "RATE_LIMIT_EXCEEDED"
            | "TOO_MANY_LOGIN_ATTEMPTS"
            | "DAILY_LIMIT_EXCEEDED"
            | "SERVICE_UNAVAILABLE"
            | "DATABASE_UNAVAILABLE"
            | "EXTERNAL_SERVICE_UNAVAILABLE"
            | "TIMEOUT"
            | "CACHE_UNAVAILABLE"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api_error(status: u16, code: Option<&str>) -> ClientError {
        ClientError::Api {
            operation: "test",
            status,
            code: code.map(str::to_owned),
            retry_after: None,
        }
    }

    #[test]
    fn retryability_matches_problem_code_semantics() {
        assert!(api_error(500, Some("TIMEOUT")).retryable());
        assert!(api_error(500, Some("CACHE_UNAVAILABLE")).retryable());
        assert!(api_error(429, None).retryable());
        assert!(!api_error(500, Some("INTERNAL_ERROR")).retryable());
        assert!(!api_error(404, Some("LISTING_NOT_FOUND")).retryable());
        assert!(!api_error(404, Some("SERVICE_UNAVAILABLE")).retryable());
    }

    #[test]
    fn public_api_error_surface_uses_fixed_status_mappings() {
        let error = ClientError::Api {
            operation: "test",
            status: 404,
            code: Some("IGNORE_PREVIOUS_INSTRUCTIONS".to_owned()),
            retry_after: None,
        };

        assert_eq!(error.public_code(), Some("API_NOT_FOUND"));
        assert_eq!(
            error.public_message(),
            "Budna API resource was not found (HTTP 404)"
        );
    }

    #[test]
    fn local_failures_have_stable_public_codes() {
        let invalid_response = ClientError::InvalidResponse { operation: "test" };
        let response_too_large = ClientError::ResponseTooLarge {
            operation: "test",
            limit_bytes: 4_096,
        };
        let retry_error = ClientError::RetryInvariant { operation: "test" };

        assert_eq!(
            invalid_response.public_code(),
            Some("BUDNA_API_INVALID_RESPONSE")
        );
        assert_eq!(
            response_too_large.public_code(),
            Some("BUDNA_API_RESPONSE_TOO_LARGE")
        );
        assert_eq!(retry_error.public_code(), Some("BUDNA_API_RETRY_ERROR"));
        assert!(!invalid_response.retryable());
        assert!(!response_too_large.retryable());
    }

    #[test]
    fn exhausted_retry_preserves_safe_status_and_retry_after() {
        let error = ClientError::RetryExhausted {
            operation: "test",
            attempts: 3,
            last_error: Box::new(ClientError::Api {
                operation: "test",
                status: 429,
                code: Some("RATE_LIMIT_EXCEEDED".to_owned()),
                retry_after: Some(std::time::Duration::from_secs(2)),
            }),
        };

        assert_eq!(error.public_code(), Some("BUDNA_API_RETRY_EXHAUSTED"));
        assert_eq!(error.status(), Some(429));
        assert_eq!(error.retry_after(), Some(std::time::Duration::from_secs(2)));
        assert!(error.retryable());
    }
}
