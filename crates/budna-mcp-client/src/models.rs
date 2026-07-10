use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub struct Money {
    pub amount: String,
    pub currency_code: String,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub struct SearchListingsRequest {
    #[serde(rename = "q", skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listing_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ending_soon: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub featured: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_shipping: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<String>,
    pub page: u32,
    #[serde(rename = "per_page")]
    pub limit: u32,
    pub include_facets: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_municipality: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_pickup: Option<bool>,
    #[serde(flatten)]
    pub custom_filters: BTreeMap<String, String>,
}

impl Default for SearchListingsRequest {
    fn default() -> Self {
        Self {
            query: None,
            category_id: None,
            market: None,
            min_price: None,
            max_price: None,
            condition: None,
            listing_type: None,
            status: None,
            ending_soon: None,
            featured: None,
            free_shipping: None,
            sort_by: None,
            sort_order: None,
            page: 1,
            limit: 10,
            include_facets: false,
            search_mode: None,
            location_id: None,
            location_region: None,
            location_municipality: None,
            allow_pickup: None,
            custom_filters: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
pub struct ListingSearchResult {
    pub hits: Vec<SearchListingHit>,
    pub total: u64,
    pub page: u32,
    pub per_page: u32,
    pub total_pages: u32,
    pub search_time_ms: u64,
    pub facets: Option<SearchFacets>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
pub struct SearchListingHit {
    pub id: i64,
    pub seller_id: i64,
    pub title: Option<String>,
    pub category_id: Option<i32>,
    pub category_name: Option<String>,
    pub category_breadcrumb: Option<String>,
    pub condition: String,
    pub listing_type: String,
    pub currency: String,
    pub market: String,
    pub starting_price: String,
    pub current_bid: Option<String>,
    pub buy_now_price: Option<String>,
    pub shipping_cost: Option<String>,
    pub free_shipping: bool,
    pub status: String,
    pub start_time: i64,
    pub end_time: i64,
    pub featured: bool,
    pub tags: Vec<String>,
    pub image_ids: Vec<String>,
    pub primary_image_id: Option<String>,
    pub ending_soon: bool,
    pub has_bids: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
pub struct SearchFacets {
    pub categories: Vec<FacetCount>,
    pub conditions: Vec<FacetCount>,
    pub listing_types: Vec<FacetCount>,
    pub markets: Vec<FacetCount>,
    pub statuses: Vec<FacetCount>,
    pub regions: Vec<FacetCount>,
    pub cities: Vec<FacetCount>,
    pub allow_pickup: Vec<FacetCount>,
    pub price_stats: Option<PriceStats>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub struct PriceStats {
    pub min: String,
    pub max: String,
    pub avg: String,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub struct FacetCount {
    pub value: String,
    pub count: u64,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
pub struct ListingResponse {
    pub id: i64,
    pub seller_id: i64,
    pub seller_name: Option<String>,
    pub seller_username: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub category_id: Option<i32>,
    pub condition: String,
    pub listing_type: String,
    pub currency: String,
    pub market: String,
    pub starting_price: Money,
    pub bid_increment: Option<Money>,
    pub current_bid: Option<Money>,
    pub reserve_price_met: bool,
    pub buy_now_price: Option<Money>,
    pub shipping_cost: Option<Money>,
    pub quantity: i32,
    pub status: String,
    pub start_time: i64,
    pub end_time: i64,
    pub views_count: i32,
    pub bid_count: Option<i64>,
    pub featured: bool,
    pub tags: Vec<String>,
    pub image_ids: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub package_size: Option<String>,
    pub package_weight_grams: Option<i32>,
    pub shipping_provider_codes: Option<Vec<String>>,
    pub location: Option<ListingLocation>,
    pub allow_pickup: bool,
    pub buyer_protection_config: Option<BuyerProtectionConfig>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
pub struct ListingLocation {
    pub city: String,
    pub region: Option<String>,
    pub country: String,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub struct BuyerProtectionConfig {
    pub rate_percent: String,
    pub flat_fee: String,
    pub mandatory_threshold: String,
    pub cap: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub struct CategoryListRequest {
    pub page: i32,
    pub limit: i32,
    pub parent_id: Option<i32>,
    pub include_filters: bool,
    pub translations: bool,
}

impl Default for CategoryListRequest {
    fn default() -> Self {
        Self {
            page: 1,
            limit: 100,
            parent_id: None,
            include_filters: false,
            translations: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
pub struct CategoryPage {
    pub items: Vec<CategorySummary>,
    pub pagination: Pagination,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
pub struct CategorySummary {
    pub id: i32,
    pub name: String,
    pub parent_id: Option<i32>,
    pub listing_count: i64,
    pub translations: Option<CategoryTranslations>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub struct CategoryTranslations {
    pub name: TranslationMap,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub struct TranslationMap {
    pub en: String,
    pub sv: String,
    pub no: String,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub struct Pagination {
    pub page: i64,
    pub limit: i64,
    pub total: i64,
    pub total_pages: i64,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
pub struct SellerProfileSummary {
    pub id: i64,
    pub user_id: i64,
    pub username: Option<String>,
    pub display_name: String,
    pub bio: Option<String>,
    pub language: String,
    pub currency: String,
    pub auction_history: PublicAuctionHistory,
    pub verification_status: PublicVerificationStatus,
    pub rating: String,
    pub total_ratings: i32,
    pub image_id: Option<String>,
    pub categories: Vec<String>,
    pub is_company: bool,
    pub created_at: i64,
    pub followers_count: Option<i64>,
    pub following_count: Option<i64>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub level: Option<i32>,
    pub level_name: Option<String>,
    pub unlocked_badges: Option<Vec<PublicBadge>>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub struct PublicAuctionHistory {
    pub won_auctions_count: u64,
    pub sold_items_count: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub struct PublicVerificationStatus {
    pub id_verified: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub struct PublicBadge {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub icon_url: Option<String>,
    pub unlocked_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub struct ListingBidSummary {
    pub listing_id: i64,
    pub bid_count: Option<i64>,
    pub current_bid: Option<Money>,
    pub reserve_price_met: bool,
    pub listing_status: String,
    pub end_time: i64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PublicListingWire {
    #[serde(flatten)]
    pub listing: ListingResponse,
    pub approved: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ApiEnvelope<T> {
    pub success: bool,
    pub data: Option<T>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProblemDetails {
    pub title: Option<String>,
    pub detail: Option<String>,
    pub code: Option<String>,
}
