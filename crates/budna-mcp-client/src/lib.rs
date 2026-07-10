#![cfg_attr(test, recursion_limit = "256")]

mod config;
mod error;
mod models;

use std::time::{Duration, Instant, SystemTime};

use reqwest::{
    Client, RequestBuilder,
    header::{ACCEPT, RETRY_AFTER},
    redirect::Policy,
};
use serde::de::DeserializeOwned;
use thiserror::Error;

pub use config::{ClientConfig, ClientConfigError, DEFAULT_REQUEST_TIMEOUT};
pub use error::{ClientError, RequestFailureKind};
pub use models::{
    BuyerProtectionConfig, CategoryListRequest, CategoryPage, CategorySummary,
    CategoryTranslations, FacetCount, ListingBidSummary, ListingLocation, ListingResponse,
    ListingSearchResult, Money, Pagination, PriceStats, PublicAuctionHistory, PublicBadge,
    PublicVerificationStatus, SearchFacets, SearchListingHit, SearchListingsRequest,
    SellerProfileSummary, TranslationMap,
};

use crate::models::{ApiEnvelope, ProblemDetails, PublicListingWire};

const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MAX_GET_ATTEMPTS: u32 = 3;
const INITIAL_RETRY_DELAY: Duration = Duration::from_millis(200);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(5);
const ACCEPTED_RESPONSE_MEDIA_TYPES: &str = "application/json, application/problem+json";
const MAX_PROBLEM_TITLE_CHARS: usize = 120;
const MAX_PROBLEM_DETAIL_CHARS: usize = 500;

#[derive(Clone)]
pub struct PublicApiClient {
    http: Client,
    config: ClientConfig,
}

impl PublicApiClient {
    pub fn new(config: ClientConfig) -> Result<Self, ClientBuildError> {
        let connect_timeout = config.request_timeout().min(Duration::from_secs(10));
        let http = Client::builder()
            .connect_timeout(connect_timeout)
            .timeout(config.request_timeout())
            .redirect(Policy::none())
            .user_agent(concat!("budna-mcp/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(ClientBuildError::HttpClient)?;

        Ok(Self { http, config })
    }

    pub fn base_url(&self) -> &reqwest::Url {
        self.config.base_url()
    }

    pub async fn search_listings(
        &self,
        query: SearchListingsRequest,
    ) -> Result<ListingSearchResult, ClientError> {
        const OPERATION: &str = "search_listings";
        let url = self.endpoint(OPERATION, "search/listings")?;
        let request = self.http.get(url).query(&query);
        let result: ListingSearchResult = self.get_required(OPERATION, request).await?;

        if result
            .hits
            .iter()
            .all(|hit| is_public_listing_status(&hit.status))
        {
            Ok(result)
        } else {
            Err(public_resource_unavailable(
                OPERATION,
                "SEARCH_UNAVAILABLE",
                "Search results unavailable",
            ))
        }
    }

    pub async fn get_listing(&self, listing_id: i64) -> Result<ListingResponse, ClientError> {
        const OPERATION: &str = "get_listing";
        self.get_public_listing(OPERATION, listing_id).await
    }

    pub async fn list_categories(
        &self,
        query: CategoryListRequest,
    ) -> Result<CategoryPage, ClientError> {
        const OPERATION: &str = "get_categories";
        let url = self.endpoint(OPERATION, "categories")?;
        let request = self.http.get(url).query(&query);
        self.get_required(OPERATION, request).await
    }

    pub async fn get_categories(&self) -> Result<Vec<CategorySummary>, ClientError> {
        self.list_categories(CategoryListRequest::default())
            .await
            .map(|page| page.items)
    }

    pub async fn get_public_seller_profile(
        &self,
        seller_id: i64,
    ) -> Result<SellerProfileSummary, ClientError> {
        const OPERATION: &str = "get_public_seller_profile";
        let url = self.endpoint(OPERATION, &format!("profiles/{seller_id}"))?;
        self.get_required(OPERATION, self.http.get(url)).await
    }

    pub async fn get_listing_bid_summary(
        &self,
        listing_id: i64,
    ) -> Result<ListingBidSummary, ClientError> {
        const OPERATION: &str = "get_listing_bid_summary";
        let listing = self.get_public_listing(OPERATION, listing_id).await?;

        Ok(ListingBidSummary {
            listing_id: listing.id,
            bid_count: listing.bid_count,
            current_bid: listing.current_bid,
            reserve_price_met: listing.reserve_price_met,
            listing_status: listing.status,
            end_time: listing.end_time,
        })
    }

    async fn get_public_listing(
        &self,
        operation: &'static str,
        listing_id: i64,
    ) -> Result<ListingResponse, ClientError> {
        let url = self.endpoint(operation, &format!("listings/{listing_id}"))?;
        let wire: PublicListingWire = self.get_required(operation, self.http.get(url)).await?;

        if wire.approved && is_public_listing_status(&wire.listing.status) {
            Ok(wire.listing)
        } else {
            Err(public_resource_unavailable(
                operation,
                "LISTING_NOT_FOUND",
                "Listing not found",
            ))
        }
    }

    fn endpoint(&self, operation: &'static str, path: &str) -> Result<reqwest::Url, ClientError> {
        self.config
            .endpoint(path)
            .map_err(|source| ClientError::Endpoint { operation, source })
    }

    async fn get_required<T>(
        &self,
        operation: &'static str,
        request: RequestBuilder,
    ) -> Result<T, ClientError>
    where
        T: DeserializeOwned,
    {
        self.get_optional(operation, request)
            .await?
            .ok_or(ClientError::MissingData { operation })
    }

    async fn get_optional<T>(
        &self,
        operation: &'static str,
        request: RequestBuilder,
    ) -> Result<Option<T>, ClientError>
    where
        T: DeserializeOwned,
    {
        let request = request
            .header(ACCEPT, ACCEPTED_RESPONSE_MEDIA_TYPES)
            .build()
            .map_err(|source| request_error(operation, source))?;
        let mut retry_delay = INITIAL_RETRY_DELAY;

        for attempt in 1..=MAX_GET_ATTEMPTS {
            let started = Instant::now();
            let attempt_request = request
                .try_clone()
                .ok_or(ClientError::RetryInvariant { operation })?;
            let result = self.send_once::<T>(operation, attempt_request).await;
            let elapsed_ms = elapsed_millis(started);

            match result {
                Ok(value) => {
                    tracing::debug!(
                        operation,
                        attempt,
                        elapsed_ms,
                        outcome = "success",
                        "Budna API request completed"
                    );
                    return Ok(value);
                }
                Err(error) if error.retryable() && attempt < MAX_GET_ATTEMPTS => {
                    let requested_delay = retry_after(&error);
                    if requested_delay.is_some_and(|delay| delay > MAX_RETRY_DELAY) {
                        tracing::warn!(
                            operation,
                            attempt,
                            elapsed_ms,
                            error.kind = error.kind(),
                            http.status = error.status(),
                            retry_after_ms = requested_delay.map(|delay| delay.as_millis() as u64),
                            "Budna API retry delay exceeds the interactive wait budget"
                        );
                        return Err(error);
                    }
                    let delay = requested_delay.unwrap_or(retry_delay).min(MAX_RETRY_DELAY);
                    tracing::warn!(
                        operation,
                        attempt,
                        elapsed_ms,
                        error.kind = error.kind(),
                        http.status = error.status(),
                        retry_delay_ms = delay.as_millis() as u64,
                        "Retrying Budna API request"
                    );
                    tokio::time::sleep(delay).await;
                    retry_delay = retry_delay.saturating_mul(2);
                }
                Err(error) => {
                    tracing::warn!(
                        operation,
                        attempt,
                        elapsed_ms,
                        error.kind = error.kind(),
                        http.status = error.status(),
                        "Budna API request failed"
                    );
                    return Err(error);
                }
            }
        }

        Err(ClientError::RetryInvariant { operation })
    }

    async fn send_once<T>(
        &self,
        operation: &'static str,
        request: reqwest::Request,
    ) -> Result<Option<T>, ClientError>
    where
        T: DeserializeOwned,
    {
        let mut response = self
            .http
            .execute(request)
            .await
            .map_err(|source| request_error(operation, source))?;
        let status = response.status();
        let retry_after = parse_retry_after(response.headers().get(RETRY_AFTER));

        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err(ClientError::ResponseTooLarge {
                operation,
                limit_bytes: MAX_RESPONSE_BYTES,
            });
        }

        let mut body = Vec::with_capacity(
            response
                .content_length()
                .unwrap_or_default()
                .min(MAX_RESPONSE_BYTES as u64) as usize,
        );
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|source| request_error(operation, source))?
        {
            if chunk.len() > MAX_RESPONSE_BYTES.saturating_sub(body.len()) {
                return Err(ClientError::ResponseTooLarge {
                    operation,
                    limit_bytes: MAX_RESPONSE_BYTES,
                });
            }
            body.extend_from_slice(&chunk);
        }

        if !status.is_success() {
            let problem = serde_json::from_slice::<ProblemDetails>(&body).ok();
            let code = problem
                .as_ref()
                .and_then(|value| value.code.clone())
                .and_then(sanitize_problem_code);
            let (title, detail) = if status.is_server_error() {
                (Some("Budna API unavailable".to_owned()), None)
            } else {
                (
                    problem
                        .as_ref()
                        .and_then(|value| value.title.clone())
                        .and_then(|value| sanitize_problem_text(value, MAX_PROBLEM_TITLE_CHARS)),
                    problem
                        .and_then(|value| value.detail)
                        .and_then(|value| sanitize_problem_text(value, MAX_PROBLEM_DETAIL_CHARS)),
                )
            };
            return Err(ClientError::Api {
                operation,
                status: status.as_u16(),
                code,
                title,
                detail,
                retry_after,
            });
        }

        let envelope = serde_json::from_slice::<ApiEnvelope<T>>(&body)
            .map_err(|source| ClientError::Decode { operation, source })?;
        if !envelope.success {
            return Err(ClientError::UnsuccessfulEnvelope { operation });
        }

        Ok(envelope.data)
    }
}

fn is_public_listing_status(status: &str) -> bool {
    matches!(status, "active" | "sold" | "expired")
}

fn public_resource_unavailable(
    operation: &'static str,
    code: &'static str,
    message: &'static str,
) -> ClientError {
    ClientError::PublicResourceUnavailable {
        operation,
        code,
        message,
    }
}

fn request_error(operation: &'static str, source: reqwest::Error) -> ClientError {
    let kind = if source.is_timeout() {
        RequestFailureKind::Timeout
    } else if source.is_connect() {
        RequestFailureKind::Connect
    } else {
        RequestFailureKind::Transport
    };

    ClientError::Request {
        operation,
        kind,
        source: source.without_url(),
    }
}

fn parse_retry_after(value: Option<&reqwest::header::HeaderValue>) -> Option<Duration> {
    parse_retry_after_at(value, SystemTime::now())
}

fn parse_retry_after_at(
    value: Option<&reqwest::header::HeaderValue>,
    now: SystemTime,
) -> Option<Duration> {
    let value = value.and_then(|value| value.to_str().ok())?;
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }

    let retry_at = httpdate::parse_http_date(value).ok()?;
    Some(retry_at.duration_since(now).unwrap_or(Duration::ZERO))
}

fn retry_after(error: &ClientError) -> Option<Duration> {
    match error {
        ClientError::Api { retry_after, .. } => *retry_after,
        _ => None,
    }
}

fn elapsed_millis(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

fn sanitize_problem_code(value: String) -> Option<String> {
    let mut bytes = value.bytes();
    let valid = value.len() <= 64
        && bytes.next().is_some_and(|byte| byte.is_ascii_uppercase())
        && bytes.all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_');
    valid.then_some(value)
}

fn sanitize_problem_text(value: String, max_chars: usize) -> Option<String> {
    let cleaned = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let normalized = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }

    Some(normalized.chars().take(max_chars).collect())
}

#[derive(Debug, Error)]
pub enum ClientBuildError {
    #[error("failed to build the Budna HTTP client: {0}")]
    HttpClient(#[source] reqwest::Error),
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{headers, method, path, query_param},
    };

    use super::*;

    fn client(server: &MockServer) -> PublicApiClient {
        let config = ClientConfig::new(server.uri()).unwrap_or_else(|error| {
            panic!("mock URL should be valid: {error}");
        });
        PublicApiClient::new(config).unwrap_or_else(|error| {
            panic!("client should build: {error}");
        })
    }

    fn listing_envelope(
        listing_id: i64,
        approved: bool,
        status: &str,
        bid_count: Option<i64>,
    ) -> serde_json::Value {
        json!({
            "success": true,
            "data": {
                "id": listing_id,
                "seller_id": 42,
                "seller_name": "Public seller",
                "seller_username": "seller42",
                "title": "Camera",
                "description": "A public listing",
                "category_id": 12,
                "condition": "good",
                "listing_type": "auction",
                "currency": "NOK",
                "market": "norwegian",
                "starting_price": {"amount": "100.00", "currency_code": "NOK"},
                "bid_increment": {"amount": "10.00", "currency_code": "NOK"},
                "current_bid": {"amount": "120.00", "currency_code": "NOK"},
                "reserve_price_met": false,
                "buy_now_price": null,
                "shipping_cost": {"amount": "49.00", "currency_code": "NOK"},
                "quantity": 1,
                "status": status,
                "start_time": 1_700_000_000_000_i64,
                "end_time": 1_800_000_000_000_i64,
                "views_count": 20,
                "bid_count": bid_count,
                "approved": approved,
                "featured": false,
                "tags": ["camera"],
                "image_ids": ["123e4567-e89b-12d3-a456-426614174000"],
                "created_at": 1_700_000_000_000_i64,
                "updated_at": 1_700_000_000_100_i64,
                "package_size": "medium",
                "package_weight_grams": 900,
                "shipping_provider_codes": ["postnord"],
                "location": {
                    "city": "Oslo",
                    "region": "Oslo",
                    "country": "NO",
                    "server_only_marker": "ignored"
                },
                "allow_pickup": true,
                "buyer_protection_config": {
                    "rate_percent": "5.0",
                    "flat_fee": "10.00",
                    "mandatory_threshold": "100.00",
                    "cap": "500.00",
                    "enabled": true
                },
                "server_only_marker": "ignored"
            }
        })
    }

    fn search_hit(listing_id: i64, status: &str) -> serde_json::Value {
        json!({
            "id": listing_id,
            "seller_id": 42,
            "title": "Camera",
            "category_id": 12,
            "category_name": "Cameras",
            "category_breadcrumb": "Electronics > Cameras",
            "condition": "good",
            "listing_type": "auction",
            "currency": "NOK",
            "market": "norwegian",
            "starting_price": "100.00",
            "current_bid": "120.00",
            "buy_now_price": null,
            "shipping_cost": "49.00",
            "free_shipping": false,
            "status": status,
            "start_time": 1_700_000_000_000_i64,
            "end_time": 1_800_000_000_000_i64,
            "featured": false,
            "tags": ["camera"],
            "image_ids": ["123e4567-e89b-12d3-a456-426614174000"],
            "primary_image_id": "123e4567-e89b-12d3-a456-426614174000",
            "ending_soon": false,
            "has_bids": true,
            "server_only_marker": "ignored"
        })
    }

    #[tokio::test]
    async fn search_uses_api_v1_query_contract_and_unwraps_data() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/search/listings"))
            .and(query_param("q", "camera lens"))
            .and(query_param("category_id", "12"))
            .and(query_param("market", "norwegian"))
            .and(query_param("min_price", "10"))
            .and(query_param("max_price", "999"))
            .and(query_param("condition", "good"))
            .and(query_param("listing_type", "auction"))
            .and(query_param("status", "active"))
            .and(query_param("ending_soon", "true"))
            .and(query_param("featured", "false"))
            .and(query_param("free_shipping", "true"))
            .and(query_param("sort_by", "price"))
            .and(query_param("sort_order", "asc"))
            .and(query_param("page", "2"))
            .and(query_param("per_page", "7"))
            .and(query_param("include_facets", "true"))
            .and(query_param("search_mode", "hybrid"))
            .and(query_param("location_id", "9"))
            .and(query_param("location_region", "Oslo"))
            .and(query_param("location_municipality", "Oslo"))
            .and(query_param("allow_pickup", "true"))
            .and(query_param("attr_mount", "sony-e"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "data": {
                    "hits": [],
                    "total": 0,
                    "page": 2,
                    "per_page": 7,
                    "total_pages": 0,
                    "search_time_ms": 3,
                    "facets": null
                }
            })))
            .mount(&server)
            .await;

        let result = client(&server)
            .search_listings(SearchListingsRequest {
                query: Some("camera lens".to_owned()),
                category_id: Some(12),
                market: Some("norwegian".to_owned()),
                min_price: Some("10".to_owned()),
                max_price: Some("999".to_owned()),
                condition: Some("good".to_owned()),
                listing_type: Some("auction".to_owned()),
                status: Some("active".to_owned()),
                ending_soon: Some(true),
                featured: Some(false),
                free_shipping: Some(true),
                sort_by: Some("price".to_owned()),
                sort_order: Some("asc".to_owned()),
                page: 2,
                limit: 7,
                include_facets: true,
                search_mode: Some("hybrid".to_owned()),
                location_id: Some(9),
                location_region: Some("Oslo".to_owned()),
                location_municipality: Some("Oslo".to_owned()),
                allow_pickup: Some(true),
                custom_filters: BTreeMap::from([("attr_mount".to_owned(), "sony-e".to_owned())]),
            })
            .await
            .unwrap_or_else(|error| panic!("search should succeed: {error}"));

        assert_eq!(result.page, 2);
        assert_eq!(result.per_page, 7);
    }

    #[tokio::test]
    async fn search_fails_closed_for_unexpected_visibility_state() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/search/listings"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "data": {
                    "hits": [search_hit(7, "not_public")],
                    "total": 1,
                    "page": 1,
                    "per_page": 10,
                    "total_pages": 1,
                    "search_time_ms": 3,
                    "facets": null
                }
            })))
            .mount(&server)
            .await;

        let error = match client(&server)
            .search_listings(SearchListingsRequest::default())
            .await
        {
            Ok(_) => panic!("non-public search result should fail closed"),
            Err(error) => error,
        };

        assert_eq!(error.status(), Some(404));
        assert_eq!(error.code(), Some("SEARCH_UNAVAILABLE"));
        assert_eq!(
            error.public_message(),
            "Search results unavailable (HTTP 404)"
        );
    }

    #[tokio::test]
    async fn problem_response_becomes_typed_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/listings/404"))
            .and(headers(
                "accept",
                vec!["application/json", "application/problem+json"],
            ))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "type": "https://api.budna.se/problems/resource/listing-not-found",
                "title": "Not Found",
                "status": 404,
                "code": "LISTING_NOT_FOUND",
                "detail": "Listing not found"
            })))
            .mount(&server)
            .await;

        let error = match client(&server).get_listing(404).await {
            Ok(_) => panic!("missing listing should fail"),
            Err(error) => error,
        };

        assert_eq!(error.status(), Some(404));
        assert_eq!(error.code(), Some("LISTING_NOT_FOUND"));
        assert!(!error.retryable());
        assert_eq!(
            error.public_message(),
            "Not Found (HTTP 404): Listing not found"
        );
    }

    #[tokio::test]
    async fn server_error_details_are_not_relayed() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/listings/500"))
            .respond_with(ResponseTemplate::new(500).set_body_json(json!({
                "type": "about:blank",
                "title": "Storage connection details",
                "status": 500,
                "code": "INTERNAL_ERROR",
                "detail": "sensitive diagnostic text"
            })))
            .mount(&server)
            .await;

        let error = match client(&server).get_listing(500).await {
            Ok(_) => panic!("server error should fail"),
            Err(error) => error,
        };

        assert_eq!(error.code(), Some("INTERNAL_ERROR"));
        assert_eq!(error.public_message(), "Budna API unavailable (HTTP 500)");
    }

    #[test]
    fn problem_fields_are_bounded_and_normalized() {
        assert_eq!(
            sanitize_problem_code("VALIDATION_FAILED".to_owned()).as_deref(),
            Some("VALIDATION_FAILED")
        );
        assert!(sanitize_problem_code("not-valid".to_owned()).is_none());
        assert!(sanitize_problem_code(format!("A{}", "B".repeat(64))).is_none());
        assert_eq!(
            sanitize_problem_text("  Bad\n\trequest\u{0000}  ".to_owned(), 7).as_deref(),
            Some("Bad req")
        );
    }

    #[tokio::test]
    async fn public_listing_contract_decodes_and_unknown_bid_count_is_preserved() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/listings/7"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(listing_envelope(7, true, "active", None)),
            )
            .mount(&server)
            .await;

        let listing = client(&server)
            .get_listing(7)
            .await
            .unwrap_or_else(|error| panic!("public listing should decode: {error}"));
        assert_eq!(
            listing
                .location
                .as_ref()
                .map(|location| location.city.as_str()),
            Some("Oslo")
        );

        let summary = client(&server)
            .get_listing_bid_summary(7)
            .await
            .unwrap_or_else(|error| panic!("public bid summary should decode: {error}"));
        let summary_json = serde_json::to_value(&summary)
            .unwrap_or_else(|error| panic!("bid summary should serialize: {error}"));
        let summary_keys = summary_json
            .as_object()
            .map(|object| object.keys().map(String::as_str).collect::<BTreeSet<_>>())
            .unwrap_or_else(|| panic!("bid summary should be an object"));
        assert_eq!(
            summary_keys,
            BTreeSet::from([
                "bid_count",
                "current_bid",
                "end_time",
                "listing_id",
                "listing_status",
                "reserve_price_met",
            ])
        );
        assert_eq!(summary.bid_count, None);
        assert_eq!(
            summary.current_bid.map(|money| money.amount).as_deref(),
            Some("120.00")
        );
    }

    #[tokio::test]
    async fn non_public_listing_states_are_indistinguishable_from_not_found() {
        let server = MockServer::start().await;
        for (listing_id, approved, status) in [
            (10, false, "active"),
            (11, true, "draft"),
            (12, true, "cancelled"),
            (13, true, "unknown"),
        ] {
            Mock::given(method("GET"))
                .and(path(format!("/api/v1/listings/{listing_id}")))
                .respond_with(ResponseTemplate::new(200).set_body_json(listing_envelope(
                    listing_id,
                    approved,
                    status,
                    Some(2),
                )))
                .mount(&server)
                .await;

            let error = match client(&server).get_listing(listing_id).await {
                Ok(_) => panic!("non-public listing should be hidden"),
                Err(error) => error,
            };
            assert_eq!(error.status(), Some(404));
            assert_eq!(error.code(), Some("LISTING_NOT_FOUND"));
            assert_eq!(error.public_message(), "Listing not found (HTTP 404)");
        }
    }

    #[tokio::test]
    async fn categories_and_profiles_decode_current_public_contracts() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/categories"))
            .and(query_param("page", "1"))
            .and(query_param("limit", "100"))
            .and(query_param("parent_id", "2"))
            .and(query_param("include_filters", "false"))
            .and(query_param("translations", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "data": {
                    "items": [{
                        "id": 12,
                        "name": "Cameras",
                        "parent_id": null,
                        "created_at": 1_700_000_000_000_i64,
                        "listing_count": 4,
                        "filters": null,
                        "translations": {"name": {"en": "Cameras", "sv": "Kameror", "no": "Kameraer"}}
                    }],
                    "pagination": {"page": 1, "limit": 100, "total": 1, "total_pages": 1}
                }
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/profiles/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "data": {
                    "id": 5,
                    "user_id": 42,
                    "username": "seller42",
                    "display_name": "Public seller",
                    "bio": "Camera enthusiast",
                    "language": "norwegian",
                    "currency": "NOK",
                    "auction_history": {
                        "won_auctions_count": 4_294_967_296_u64,
                        "sold_items_count": 4_294_967_297_u64,
                        "server_only_marker": "ignored"
                    },
                    "verification_status": {
                        "id_verified": true,
                        "server_only_marker": "ignored"
                    },
                    "rating": "4.9",
                    "total_ratings": 12,
                    "image_id": null,
                    "categories": ["Cameras"],
                    "is_company": false,
                    "created_at": 1_700_000_000_000_i64,
                    "followers_count": 8,
                    "following_count": 2,
                    "city": "Oslo",
                    "country": "NO",
                    "level": 3,
                    "level_name": "Trusted",
                    "unlocked_badges": [],
                    "server_only_marker": "ignored"
                }
            })))
            .mount(&server)
            .await;

        let categories = client(&server)
            .list_categories(CategoryListRequest {
                parent_id: Some(2),
                ..CategoryListRequest::default()
            })
            .await
            .unwrap_or_else(|error| panic!("categories should decode: {error}"));
        assert_eq!(
            categories.items[0]
                .translations
                .as_ref()
                .map(|translations| translations.name.sv.as_str()),
            Some("Kameror")
        );

        let profile = client(&server)
            .get_public_seller_profile(42)
            .await
            .unwrap_or_else(|error| panic!("profile should decode: {error}"));
        assert_eq!(profile.auction_history.won_auctions_count, 4_294_967_296);
    }

    #[tokio::test]
    async fn oversized_response_is_rejected_before_deserialization() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/categories"))
            .respond_with(
                ResponseTemplate::new(200).set_body_bytes(vec![b'x'; MAX_RESPONSE_BYTES + 1]),
            )
            .mount(&server)
            .await;

        let error = match client(&server)
            .list_categories(CategoryListRequest::default())
            .await
        {
            Ok(_) => panic!("oversized response should fail"),
            Err(error) => error,
        };
        assert!(matches!(error, ClientError::ResponseTooLarge { .. }));
    }

    #[tokio::test]
    async fn transient_get_is_retried() {
        let server = MockServer::start().await;
        let transient = Mock::given(method("GET"))
            .and(path("/api/v1/categories"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .mount_as_scoped(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/categories"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "data": {
                    "items": [],
                    "pagination": {"page": 1, "limit": 100, "total": 0, "total_pages": 1}
                }
            })))
            .mount(&server)
            .await;

        let result = client(&server)
            .list_categories(CategoryListRequest::default())
            .await;
        drop(transient);

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn long_retry_after_is_returned_without_an_early_retry() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/categories"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "30"))
            .expect(1)
            .mount(&server)
            .await;

        let error = match client(&server)
            .list_categories(CategoryListRequest::default())
            .await
        {
            Ok(_) => panic!("rate-limited request should fail"),
            Err(error) => error,
        };

        assert_eq!(error.status(), Some(429));
        assert!(error.retryable());
    }

    #[test]
    fn retry_after_accepts_delta_seconds_and_http_dates() {
        let now = std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let delta = reqwest::header::HeaderValue::from_static("30");
        assert_eq!(
            parse_retry_after_at(Some(&delta), now),
            Some(Duration::from_secs(30))
        );

        let date = httpdate::fmt_http_date(now + Duration::from_secs(45));
        let date = reqwest::header::HeaderValue::try_from(date.as_str())
            .unwrap_or_else(|error| panic!("HTTP date should be a valid header: {error}"));
        assert_eq!(
            parse_retry_after_at(Some(&date), now),
            Some(Duration::from_secs(45))
        );
    }

    #[tokio::test]
    async fn missing_data_is_reported_for_required_resource() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/profiles/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "message": "No profile"
            })))
            .mount(&server)
            .await;

        let error = match client(&server).get_public_seller_profile(42).await {
            Ok(_) => panic!("missing profile data should fail"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            ClientError::MissingData {
                operation: "get_public_seller_profile"
            }
        ));
    }
}
