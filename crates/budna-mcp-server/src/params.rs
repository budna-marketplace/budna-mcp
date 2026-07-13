use std::{
    borrow::Cow,
    collections::BTreeMap,
    io::{self, Write},
    marker::PhantomData,
};

use budna_mcp_client::{CategoryListRequest, SearchListingsRequest};
use rmcp::schemars;
use serde::{Deserialize, Deserializer, de::DeserializeOwned};

const MAX_SEARCH_RESULTS: u32 = 50;
const MAX_SEARCH_PAGE: u32 = 10_000;
const MAX_SEARCH_PRICE_MAJOR_UNITS: &str = "1000000000000";
const MAX_CATEGORY_RESULTS: i32 = 200;
const MAX_CATEGORY_PAGE: i32 = 10_000;
const MAX_LISTING_PAGE_RESULTS: u32 = 50;
const MAX_LISTING_PAGE: u32 = 10_000;
const MAX_CUSTOM_FILTERS: usize = 20;
const MAX_TOOL_ARGUMENT_BYTES: usize = 64 * 1024;

struct BoundedSizeSink {
    remaining: usize,
}

impl Write for BoundedSizeSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.remaining {
            return Err(io::Error::other("serialized argument limit exceeded"));
        }
        self.remaining -= bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn tool_arguments_oversized(raw: &serde_json::Value) -> bool {
    let mut sink = BoundedSizeSink {
        remaining: MAX_TOOL_ARGUMENT_BYTES,
    };
    serde_json::to_writer(&mut sink, raw).is_err()
}

#[derive(Debug, Default, Deserialize)]
#[serde(transparent)]
pub struct CustomFiltersParam(BTreeMap<String, String>);

impl From<BTreeMap<String, String>> for CustomFiltersParam {
    fn from(filters: BTreeMap<String, String>) -> Self {
        Self(filters)
    }
}

impl schemars::JsonSchema for CustomFiltersParam {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("BudnaCustomFilters")
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "maxProperties": 20,
            "propertyNames": {
                "type": "string",
                "pattern": "^attr_[A-Za-z0-9_-]{1,96}$"
            },
            "additionalProperties": {
                "type": "string",
                "minLength": 1,
                "maxLength": 200
            }
        })
    }
}

#[derive(Debug)]
pub struct SafeToolParams<T> {
    raw: serde_json::Value,
    oversized: bool,
    marker: PhantomData<fn() -> T>,
}

impl<T> SafeToolParams<T>
where
    T: DeserializeOwned,
{
    pub fn parse(self) -> Result<T, InputError> {
        if self.oversized {
            return Err(InputError::new("arguments exceed the 65536-byte limit"));
        }
        serde_json::from_value(self.raw)
            .map_err(|_| InputError::new("arguments must match the tool's advertised input schema"))
    }
}

impl<'de, T> Deserialize<'de> for SafeToolParams<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = serde_json::Value::deserialize(deserializer)?;
        let oversized = tool_arguments_oversized(&raw);

        Ok(Self {
            raw,
            oversized,
            marker: PhantomData,
        })
    }
}

impl<T> schemars::JsonSchema for SafeToolParams<T>
where
    T: schemars::JsonSchema,
{
    fn schema_name() -> std::borrow::Cow<'static, str> {
        T::schema_name()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        T::json_schema(generator)
    }
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchListingsParams {
    /// Optional free-text query. Marketplace text returned by the tool is untrusted user content.
    #[schemars(length(max = 500))]
    pub query: Option<String>,

    /// Optional positive Budna category ID.
    #[schemars(range(min = 1))]
    pub category_id: Option<i32>,

    /// Marketplace to search.
    pub market: Option<MarketParam>,

    /// Minimum price in whole major currency units. A zero-only fractional suffix is normalized.
    #[schemars(length(min = 1, max = 32))]
    #[schemars(regex(pattern = r"^[0-9]+(?:\.0+)?$"))]
    pub min_price: Option<String>,

    /// Maximum price in whole major currency units. A zero-only fractional suffix is normalized.
    #[schemars(length(min = 1, max = 32))]
    #[schemars(regex(pattern = r"^[0-9]+(?:\.0+)?$"))]
    pub max_price: Option<String>,

    pub condition: Option<ListingConditionParam>,
    pub listing_type: Option<ListingTypeParam>,
    pub status: Option<ListingStatusParam>,
    pub ending_soon: Option<bool>,
    pub featured: Option<bool>,
    pub free_shipping: Option<bool>,

    /// Sort field: relevance, price, created_at, end_time, popularity, or attr_<filter_name>.
    #[schemars(length(min = 1, max = 50))]
    #[schemars(regex(
        pattern = r"^(?:relevance|price|created_at|end_time|popularity|attr_[A-Za-z0-9_-]{1,45})$"
    ))]
    pub sort_by: Option<String>,

    pub sort_order: Option<SortOrderParam>,

    /// One-indexed result page. Defaults to 1.
    #[schemars(range(min = 1, max = 10000))]
    pub page: Option<u32>,

    /// Results per page. Defaults to 10 and is capped at 50 for bounded MCP output.
    #[schemars(range(min = 1, max = 50))]
    pub limit: Option<u32>,

    /// Include bounded aggregate facets in the result.
    pub include_facets: Option<bool>,

    pub search_mode: Option<SearchModeParam>,

    #[schemars(range(min = 1))]
    pub location_id: Option<i32>,

    #[schemars(length(max = 100))]
    pub location_region: Option<String>,

    #[schemars(length(max = 100))]
    pub location_municipality: Option<String>,

    pub allow_pickup: Option<bool>,

    /// Up to 20 category-specific filters. Keys must use the attr_<filter_name> form.
    pub custom_filters: Option<CustomFiltersParam>,
}

impl SearchListingsParams {
    pub fn into_request(self) -> Result<SearchListingsRequest, InputError> {
        let query = trimmed(self.query);
        validate_max_chars("query", query.as_deref(), 500)?;
        validate_positive_i32("category_id", self.category_id)?;
        validate_positive_i32("location_id", self.location_id)?;

        let page = self.page.unwrap_or(1);
        if !(1..=MAX_SEARCH_PAGE).contains(&page) {
            return Err(InputError::new("page must be between 1 and 10000"));
        }

        let limit = self.limit.unwrap_or(10);
        if !(1..=MAX_SEARCH_RESULTS).contains(&limit) {
            return Err(InputError::new("limit must be between 1 and 50"));
        }

        let min_price = normalize_search_price("min_price", self.min_price)?;
        let max_price = normalize_search_price("max_price", self.max_price)?;
        if let (Some(min_price), Some(max_price)) = (&min_price, &max_price)
            && search_price_whole_units(min_price)? > search_price_whole_units(max_price)?
        {
            return Err(InputError::new(
                "min_price must not be greater than max_price",
            ));
        }

        let sort_by = trimmed(self.sort_by);
        validate_max_chars("sort_by", sort_by.as_deref(), 50)?;
        if let Some(value) = sort_by.as_deref()
            && !is_valid_sort_field(value)
        {
            return Err(InputError::new(
                "sort_by must be relevance, price, created_at, end_time, popularity, or attr_<filter_name>",
            ));
        }

        let location_region = trimmed(self.location_region);
        let location_municipality = trimmed(self.location_municipality);
        validate_max_chars("location_region", location_region.as_deref(), 100)?;
        validate_max_chars(
            "location_municipality",
            location_municipality.as_deref(),
            100,
        )?;
        validate_filter_literal("location_region", location_region.as_deref())?;
        validate_filter_literal("location_municipality", location_municipality.as_deref())?;

        let custom_filters = normalize_custom_filters(self.custom_filters.unwrap_or_default().0)?;

        Ok(SearchListingsRequest {
            query,
            category_id: self.category_id,
            market: self.market.map(MarketParam::as_str).map(str::to_owned),
            min_price,
            max_price,
            condition: self
                .condition
                .map(ListingConditionParam::as_str)
                .map(str::to_owned),
            listing_type: self
                .listing_type
                .map(ListingTypeParam::as_str)
                .map(str::to_owned),
            status: self
                .status
                .map(ListingStatusParam::as_str)
                .map(str::to_owned),
            ending_soon: self.ending_soon,
            featured: self.featured,
            free_shipping: self.free_shipping,
            sort_by,
            sort_order: self
                .sort_order
                .map(SortOrderParam::as_str)
                .map(str::to_owned),
            page,
            limit,
            include_facets: self.include_facets.unwrap_or(false),
            search_mode: self
                .search_mode
                .map(SearchModeParam::as_str)
                .map(str::to_owned),
            location_id: self.location_id,
            location_region,
            location_municipality,
            allow_pickup: self.allow_pickup,
            custom_filters,
        })
    }
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetCategoriesParams {
    /// One-indexed result page. Defaults to 1.
    #[schemars(range(min = 1, max = 10000))]
    pub page: Option<i32>,

    /// Categories per page. Defaults to 100 and is capped at 200 for bounded MCP output.
    #[schemars(range(min = 1, max = 200))]
    pub limit: Option<i32>,

    /// Optional positive parent category ID.
    #[schemars(range(min = 1))]
    pub parent_id: Option<i32>,

    /// Include localized category names. Defaults to true.
    pub translations: Option<bool>,
}

impl GetCategoriesParams {
    pub fn into_request(self) -> Result<CategoryListRequest, InputError> {
        let page = self.page.unwrap_or(1);
        if !(1..=MAX_CATEGORY_PAGE).contains(&page) {
            return Err(InputError::new("page must be between 1 and 10000"));
        }

        let limit = self.limit.unwrap_or(100);
        if !(1..=MAX_CATEGORY_RESULTS).contains(&limit) {
            return Err(InputError::new("limit must be between 1 and 200"));
        }
        validate_positive_i32("parent_id", self.parent_id)?;

        Ok(CategoryListRequest {
            page,
            limit,
            parent_id: self.parent_id,
            include_filters: false,
            translations: self.translations.unwrap_or(true),
        })
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListingIdParams {
    /// Positive Budna listing ID.
    #[schemars(range(min = 1))]
    pub listing_id: i64,
}

impl ListingIdParams {
    pub fn validate(&self) -> Result<(), InputError> {
        validate_positive_i64("listing_id", Some(self.listing_id))
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListingIdPageParams {
    /// Positive Budna listing ID.
    #[schemars(range(min = 1))]
    pub listing_id: i64,

    /// One-indexed result page. Defaults to 1.
    #[schemars(range(min = 1, max = 10000))]
    pub page: Option<u32>,

    /// Results per page. Defaults to 10 and is capped at 50 for bounded MCP output.
    #[schemars(range(min = 1, max = 50))]
    pub limit: Option<u32>,
}

impl ListingIdPageParams {
    pub fn validate(&self) -> Result<(i64, u32, u32), InputError> {
        validate_positive_i64("listing_id", Some(self.listing_id))?;
        let (page, limit) = validate_listing_page(self.page, self.limit)?;
        Ok((self.listing_id, page, limit))
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SellerIdParams {
    /// Positive Budna seller user ID (not the profile ID).
    #[schemars(range(min = 1))]
    pub seller_id: i64,
}

impl SellerIdParams {
    pub fn validate(&self) -> Result<(), InputError> {
        validate_positive_i64("seller_id", Some(self.seller_id))
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SellerListingsParams {
    /// Positive Budna seller user ID (not the profile ID).
    #[schemars(range(min = 1))]
    pub seller_id: i64,

    /// One-indexed result page. Defaults to 1.
    #[schemars(range(min = 1, max = 10000))]
    pub page: Option<u32>,

    /// Results per page. Defaults to 10 and is capped at 50 for bounded MCP output.
    #[schemars(range(min = 1, max = 50))]
    pub limit: Option<u32>,
}

impl SellerListingsParams {
    pub fn validate(&self) -> Result<(i64, u32, u32), InputError> {
        validate_positive_i64("seller_id", Some(self.seller_id))?;
        let (page, limit) = validate_listing_page(self.page, self.limit)?;
        Ok((self.seller_id, page, limit))
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CategoryFiltersParams {
    /// Positive Budna category ID.
    #[schemars(range(min = 1))]
    pub category_id: i32,

    /// Include localized filter labels. Defaults to true.
    pub translations: Option<bool>,
}

impl CategoryFiltersParams {
    pub fn validate(&self) -> Result<(i32, bool), InputError> {
        validate_positive_i32("category_id", Some(self.category_id))?;
        Ok((self.category_id, self.translations.unwrap_or(true)))
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FilterOptionsParams {
    /// Positive Budna filter definition ID.
    #[schemars(range(min = 1))]
    pub filter_id: i32,

    /// Include localized option labels. Defaults to true.
    pub translations: Option<bool>,
}

impl FilterOptionsParams {
    pub fn validate(&self) -> Result<(i32, bool), InputError> {
        validate_positive_i32("filter_id", Some(self.filter_id))?;
        Ok((self.filter_id, self.translations.unwrap_or(true)))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarketParam {
    Norwegian,
    Swedish,
}

impl MarketParam {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Norwegian => "norwegian",
            Self::Swedish => "swedish",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ListingConditionParam {
    New,
    LikeNew,
    VeryGood,
    Good,
    Acceptable,
}

impl ListingConditionParam {
    const fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::LikeNew => "like_new",
            Self::VeryGood => "very_good",
            Self::Good => "good",
            Self::Acceptable => "acceptable",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ListingTypeParam {
    Auction,
    FixedPrice,
    AuctionFixedPrice,
}

impl ListingTypeParam {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Auction => "auction",
            Self::FixedPrice => "fixed_price",
            Self::AuctionFixedPrice => "auction_fixed_price",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ListingStatusParam {
    Active,
    Sold,
    Expired,
}

impl ListingStatusParam {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Sold => "sold",
            Self::Expired => "expired",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SortOrderParam {
    Asc,
    Desc,
}

impl SortOrderParam {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchModeParam {
    Keyword,
    Semantic,
    Hybrid,
}

impl SearchModeParam {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Keyword => "keyword",
            Self::Semantic => "semantic",
            Self::Hybrid => "hybrid",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct InputError {
    message: String,
}

impl InputError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

fn trimmed(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

fn validate_positive_i32(field: &str, value: Option<i32>) -> Result<(), InputError> {
    if value.is_some_and(|value| value < 1) {
        return Err(InputError::new(format!("{field} must be at least 1")));
    }
    Ok(())
}

fn validate_positive_i64(field: &str, value: Option<i64>) -> Result<(), InputError> {
    if value.is_some_and(|value| value < 1) {
        return Err(InputError::new(format!("{field} must be at least 1")));
    }
    Ok(())
}

fn validate_listing_page(page: Option<u32>, limit: Option<u32>) -> Result<(u32, u32), InputError> {
    let page = page.unwrap_or(1);
    if !(1..=MAX_LISTING_PAGE).contains(&page) {
        return Err(InputError::new("page must be between 1 and 10000"));
    }

    let limit = limit.unwrap_or(10);
    if !(1..=MAX_LISTING_PAGE_RESULTS).contains(&limit) {
        return Err(InputError::new("limit must be between 1 and 50"));
    }

    Ok((page, limit))
}

fn validate_max_chars(field: &str, value: Option<&str>, max: usize) -> Result<(), InputError> {
    if value.is_some_and(|value| value.chars().count() > max) {
        return Err(InputError::new(format!(
            "{field} must contain at most {max} characters"
        )));
    }
    Ok(())
}

fn normalize_search_price(
    field: &str,
    value: Option<String>,
) -> Result<Option<String>, InputError> {
    let Some(value) = trimmed(value) else {
        return Ok(None);
    };

    if value.len() > 32 {
        return Err(search_price_error(field));
    }

    let (whole, fraction) = match value.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (value.as_str(), None),
    };
    if whole.is_empty() || !whole.chars().all(|character| character.is_ascii_digit()) {
        return Err(search_price_error(field));
    }
    if let Some(fraction) = fraction
        && (fraction.is_empty()
            || !fraction
                .chars()
                .all(|character| character.is_ascii_digit() && character == '0'))
    {
        return Err(search_price_error(field));
    }

    let normalized = whole.trim_start_matches('0');
    let normalized = if normalized.is_empty() {
        "0".to_owned()
    } else {
        normalized.to_owned()
    };
    if normalized.len() > MAX_SEARCH_PRICE_MAJOR_UNITS.len()
        || (normalized.len() == MAX_SEARCH_PRICE_MAJOR_UNITS.len()
            && normalized.as_str() > MAX_SEARCH_PRICE_MAJOR_UNITS)
    {
        return Err(search_price_error(field));
    }
    Ok(Some(normalized))
}

fn search_price_whole_units(value: &str) -> Result<u64, InputError> {
    value
        .parse::<u64>()
        .map_err(|_| search_price_error("price"))
}

fn search_price_error(field: &str) -> InputError {
    InputError::new(format!(
        "{field} must use whole major currency units no greater than {MAX_SEARCH_PRICE_MAJOR_UNITS}"
    ))
}

fn is_valid_sort_field(value: &str) -> bool {
    matches!(
        value,
        "relevance" | "price" | "created_at" | "end_time" | "popularity"
    ) || value
        .strip_prefix("attr_")
        .is_some_and(is_valid_filter_name)
}

fn normalize_custom_filters(
    filters: BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, InputError> {
    if filters.len() > MAX_CUSTOM_FILTERS {
        return Err(InputError::new(
            "custom_filters supports at most 20 entries",
        ));
    }

    let mut normalized = BTreeMap::new();
    for (key, value) in filters {
        let Some(name) = key.strip_prefix("attr_") else {
            return Err(InputError::new("custom filter keys must start with attr_"));
        };
        if !is_valid_filter_name(name) {
            return Err(InputError::new(
                "custom filter names may contain only letters, numbers, underscores, and hyphens",
            ));
        }
        let value = value.trim();
        if value.is_empty() || value.chars().count() > 200 {
            return Err(InputError::new(
                "custom filter values must contain between 1 and 200 characters",
            ));
        }
        if key.ends_with("-min") || key.ends_with("-max") {
            let range_value =
                normalize_search_price("custom filter range", Some(value.to_owned()))?
                    .ok_or_else(|| InputError::new("custom filter range cannot be empty"))?;
            normalized.insert(key, range_value);
            continue;
        }

        let mut values = Vec::new();
        for part in value.split(',') {
            let part = part.trim();
            if part.is_empty() || !is_safe_filter_literal(part) {
                return Err(InputError::new(
                    "custom filter values contain unsupported search syntax",
                ));
            }
            values.push(part);
        }
        normalized.insert(key, values.join(","));
    }
    Ok(normalized)
}

fn validate_filter_literal(field: &str, value: Option<&str>) -> Result<(), InputError> {
    if value.is_some_and(|value| !is_safe_filter_literal(value)) {
        return Err(InputError::new(format!(
            "{field} contains unsupported search syntax"
        )));
    }
    Ok(())
}

fn is_safe_filter_literal(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_alphanumeric()
                || character.is_whitespace()
                || matches!(character, '_' | '-' | '.' | '/' | '+' | '\'')
        })
}

fn is_valid_filter_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_defaults_are_bounded() {
        let request = SearchListingsParams::default()
            .into_request()
            .unwrap_or_else(|error| panic!("defaults should validate: {}", error.message()));

        assert_eq!(request.page, 1);
        assert_eq!(request.limit, 10);
        assert!(!request.include_facets);
    }

    #[test]
    fn invalid_ids_limits_and_filters_are_rejected() {
        let invalid_limit = SearchListingsParams {
            limit: Some(51),
            ..SearchListingsParams::default()
        };
        assert!(invalid_limit.into_request().is_err());

        let invalid_category = SearchListingsParams {
            category_id: Some(0),
            ..SearchListingsParams::default()
        };
        assert!(invalid_category.into_request().is_err());

        let invalid_filter = SearchListingsParams {
            custom_filters: Some(
                BTreeMap::from([("category_id".to_owned(), "1".to_owned())]).into(),
            ),
            ..SearchListingsParams::default()
        };
        assert!(invalid_filter.into_request().is_err());
    }

    #[test]
    fn category_pages_are_bounded() {
        let invalid_page = GetCategoriesParams {
            page: Some(MAX_CATEGORY_PAGE + 1),
            ..GetCategoriesParams::default()
        };

        let error = match invalid_page.into_request() {
            Ok(_) => panic!("oversized category page should fail"),
            Err(error) => error,
        };
        assert_eq!(error.message(), "page must be between 1 and 10000");
    }

    #[test]
    fn listing_pages_and_public_filter_ids_are_bounded() {
        let related = ListingIdPageParams {
            listing_id: 7,
            page: None,
            limit: None,
        }
        .validate()
        .unwrap_or_else(|error| panic!("defaults should validate: {}", error.message()));
        assert_eq!(related, (7, 1, 10));

        let invalid_related = ListingIdPageParams {
            listing_id: 7,
            page: Some(MAX_LISTING_PAGE + 1),
            limit: None,
        };
        assert!(invalid_related.validate().is_err());

        let invalid_seller = SellerListingsParams {
            seller_id: 0,
            page: None,
            limit: Some(1),
        };
        assert!(invalid_seller.validate().is_err());

        let invalid_filter = FilterOptionsParams {
            filter_id: 0,
            translations: None,
        };
        assert!(invalid_filter.validate().is_err());
    }

    #[test]
    fn whole_price_filters_are_normalized() {
        let params = SearchListingsParams {
            min_price: Some("0010.00".to_owned()),
            max_price: Some("2000.00".to_owned()),
            ..SearchListingsParams::default()
        };

        let request = params
            .into_request()
            .unwrap_or_else(|error| panic!("whole prices should validate: {}", error.message()));
        assert_eq!(request.min_price.as_deref(), Some("10"));
        assert_eq!(request.max_price.as_deref(), Some("2000"));
    }

    #[test]
    fn invalid_price_ranges_and_filter_syntax_are_rejected() {
        let fractional = SearchListingsParams {
            min_price: Some("10.50".to_owned()),
            ..SearchListingsParams::default()
        };
        assert!(fractional.into_request().is_err());

        let overflowing = SearchListingsParams {
            min_price: Some("1000000000001".to_owned()),
            ..SearchListingsParams::default()
        };
        assert!(overflowing.into_request().is_err());

        let reversed = SearchListingsParams {
            min_price: Some("21".to_owned()),
            max_price: Some("20".to_owned()),
            ..SearchListingsParams::default()
        };
        assert!(reversed.into_request().is_err());

        let injected = SearchListingsParams {
            custom_filters: Some(
                BTreeMap::from([(
                    "attr_color".to_owned(),
                    "red || unexpected:=true".to_owned(),
                )])
                .into(),
            ),
            ..SearchListingsParams::default()
        };
        assert!(injected.into_request().is_err());
    }

    #[test]
    fn search_schema_advertises_dynamic_filter_and_sort_constraints() {
        let schema = serde_json::to_value(schemars::schema_for!(SearchListingsParams))
            .unwrap_or_else(|error| panic!("schema should serialize: {error}"));

        assert_eq!(
            schema.pointer("/properties/custom_filters/maxProperties"),
            Some(&serde_json::json!(20))
        );
        assert_eq!(
            schema.pointer("/properties/custom_filters/propertyNames/pattern"),
            Some(&serde_json::json!("^attr_[A-Za-z0-9_-]{1,96}$"))
        );
        assert_eq!(
            schema.pointer("/properties/custom_filters/additionalProperties/maxLength"),
            Some(&serde_json::json!(200))
        );
        assert!(
            schema
                .pointer("/properties/sort_by/pattern")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|pattern| pattern.contains("attr_"))
        );
    }

    #[test]
    fn safe_tool_params_bound_stdio_argument_size_before_typed_parsing() {
        let oversized = serde_json::json!({
            "query": "x".repeat(MAX_TOOL_ARGUMENT_BYTES + 1)
        });

        let params = serde_json::from_value::<SafeToolParams<SearchListingsParams>>(oversized)
            .unwrap_or_else(|error| panic!("raw tool arguments should deserialize: {error}"));

        let error = params.parse().unwrap_err();
        assert_eq!(error.message(), "arguments exceed the 65536-byte limit");
    }
}
