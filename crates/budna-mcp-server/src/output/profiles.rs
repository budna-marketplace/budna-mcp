use budna_mcp_client::{PublicBadge, RatingSummary, SellerProfileSummary};
use rmcp::schemars;
use serde::Serialize;

use super::{
    MAX_CODE_BYTES, MAX_DISPLAY_TEXT_BYTES, MAX_LONG_TEXT_BYTES, MAX_NAME_BYTES, ProjectionBudget,
    UNTRUSTED_CONTENT_NOTICE,
};

const MAX_SELLER_CATEGORIES: usize = 50;
const MAX_SELLER_BADGES: usize = 50;

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SellerProfileOutput {
    pub content_notice: String,
    pub truncated: bool,
    pub profile_id: i64,
    pub seller_id: i64,
    #[schemars(length(max = 256))]
    pub username: Option<String>,
    #[schemars(length(max = 256))]
    pub display_name: String,
    #[schemars(length(max = 4096))]
    pub bio: Option<String>,
    #[schemars(length(max = 128))]
    pub language: String,
    #[schemars(length(min = 3, max = 3), regex(pattern = "^[A-Z]{3}$"))]
    pub currency: String,
    pub won_auctions_count: u64,
    pub sold_items_count: u64,
    pub identity_verified: bool,
    #[schemars(length(max = 128))]
    pub rating: String,
    pub total_ratings: i32,
    #[schemars(length(max = 128))]
    pub image_id: Option<String>,
    #[schemars(length(max = 50), inner(length(max = 512)))]
    pub categories: Vec<String>,
    pub is_company: bool,
    pub created_at: i64,
    pub followers_count: Option<i64>,
    pub following_count: Option<i64>,
    #[schemars(length(max = 256))]
    pub city: Option<String>,
    #[schemars(length(max = 128))]
    pub country: Option<String>,
    pub level: Option<i32>,
    #[schemars(length(max = 256))]
    pub level_name: Option<String>,
    #[schemars(length(max = 50))]
    pub badges: Option<Vec<BadgeOutput>>,
}

impl From<SellerProfileSummary> for SellerProfileOutput {
    fn from(profile: SellerProfileSummary) -> Self {
        let mut budget = ProjectionBudget::default();
        let username = budget.optional_text(profile.username, MAX_NAME_BYTES);
        let display_name = budget.text(profile.display_name, MAX_NAME_BYTES);
        let bio = budget.optional_text(profile.bio, MAX_LONG_TEXT_BYTES);
        let language = budget.text(profile.language, MAX_CODE_BYTES);
        let rating = budget.text(profile.rating, MAX_CODE_BYTES);
        let image_id = budget.optional_text(profile.image_id, MAX_CODE_BYTES);
        let categories = budget.strings(
            profile.categories,
            MAX_SELLER_CATEGORIES,
            MAX_DISPLAY_TEXT_BYTES,
        );
        let city = budget.optional_text(profile.city, MAX_NAME_BYTES);
        let country = budget.optional_text(profile.country, MAX_CODE_BYTES);
        let level_name = budget.optional_text(profile.level_name, MAX_NAME_BYTES);
        let badges = profile.unlocked_badges.map(|badges| {
            budget
                .objects(badges, MAX_SELLER_BADGES)
                .into_iter()
                .map(|badge| BadgeOutput::project(badge, &mut budget))
                .collect()
        });
        let truncated = budget.truncated();

        Self {
            content_notice: UNTRUSTED_CONTENT_NOTICE.to_owned(),
            truncated,
            profile_id: profile.id,
            seller_id: profile.user_id,
            username,
            display_name,
            bio,
            language,
            currency: profile.currency,
            won_auctions_count: profile.auction_history.won_auctions_count,
            sold_items_count: profile.auction_history.sold_items_count,
            identity_verified: profile.verification_status.id_verified,
            rating,
            total_ratings: profile.total_ratings,
            image_id,
            categories,
            is_company: profile.is_company,
            created_at: profile.created_at,
            followers_count: profile.followers_count,
            following_count: profile.following_count,
            city,
            country,
            level: profile.level,
            level_name,
            badges,
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct BadgeOutput {
    #[schemars(length(max = 128))]
    pub slug: String,
    #[schemars(length(max = 256))]
    pub name: String,
    #[schemars(length(max = 512))]
    pub description: Option<String>,
    #[schemars(length(max = 128))]
    pub category: Option<String>,
    pub unlocked_at: Option<i64>,
}

impl BadgeOutput {
    fn project(badge: PublicBadge, budget: &mut ProjectionBudget) -> Self {
        Self {
            slug: budget.text(badge.slug, MAX_CODE_BYTES),
            name: budget.text(badge.name, MAX_NAME_BYTES),
            description: budget.optional_text(badge.description, MAX_DISPLAY_TEXT_BYTES),
            category: budget.optional_text(badge.category, MAX_CODE_BYTES),
            unlocked_at: badge.unlocked_at,
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct RatingSummaryOutput {
    pub truncated: bool,
    pub listing_id: i64,
    pub total_ratings: i64,
    pub average_rating: f64,
    pub rating_distribution: [i64; 5],
    pub total_comments: i64,
    pub has_ratings: bool,
    pub has_comments: bool,
    pub most_common_rating: Option<i32>,
    pub positive_percentage: f64,
}

impl From<RatingSummary> for RatingSummaryOutput {
    fn from(summary: RatingSummary) -> Self {
        Self {
            truncated: false,
            listing_id: summary.listing_id,
            total_ratings: summary.total_ratings,
            average_rating: summary.average_rating,
            rating_distribution: summary.rating_distribution,
            total_comments: summary.total_comments,
            has_ratings: summary.has_ratings,
            has_comments: summary.has_comments,
            most_common_rating: summary.most_common_rating,
            positive_percentage: summary.positive_percentage,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use budna_mcp_client::{PublicAuctionHistory, PublicVerificationStatus};

    use super::*;
    use crate::output::assert_within_mcp_result_budget;

    fn keys(value: &serde_json::Value) -> BTreeSet<&str> {
        value
            .as_object()
            .map(|object| object.keys().map(String::as_str).collect())
            .unwrap_or_else(|| panic!("expected an object"))
    }

    fn profile_fixture() -> SellerProfileSummary {
        SellerProfileSummary {
            id: 5,
            user_id: 42,
            username: Some("seller42".to_owned()),
            display_name: "Public seller".to_owned(),
            bio: Some("Camera enthusiast".to_owned()),
            language: "norwegian".to_owned(),
            currency: "NOK".to_owned(),
            auction_history: PublicAuctionHistory {
                won_auctions_count: 4,
                sold_items_count: 9,
            },
            verification_status: PublicVerificationStatus { id_verified: true },
            rating: "4.9".to_owned(),
            total_ratings: 12,
            image_id: None,
            categories: vec!["Cameras".to_owned()],
            is_company: false,
            created_at: 1_700_000_000_000,
            followers_count: Some(8),
            following_count: Some(2),
            city: Some("Oslo".to_owned()),
            country: Some("NO".to_owned()),
            level: Some(3),
            level_name: Some("Trusted".to_owned()),
            unlocked_badges: Some(vec![PublicBadge {
                slug: "trusted-seller".to_owned(),
                name: "Trusted seller".to_owned(),
                description: Some("Completed public verification".to_owned()),
                category: Some("trust".to_owned()),
                icon_url: Some("https://untrusted.example.test/icon.svg".to_owned()),
                unlocked_at: Some(1_700_000_000_000),
            }]),
        }
    }

    #[test]
    fn seller_profile_and_badge_shapes_are_exact_and_omit_icon_urls() {
        let output = SellerProfileOutput::from(profile_fixture());
        let value = serde_json::to_value(&output)
            .unwrap_or_else(|error| panic!("profile should serialize: {error}"));
        assert_eq!(
            keys(&value),
            BTreeSet::from([
                "badges",
                "bio",
                "categories",
                "city",
                "content_notice",
                "country",
                "created_at",
                "currency",
                "display_name",
                "followers_count",
                "following_count",
                "identity_verified",
                "image_id",
                "is_company",
                "language",
                "level",
                "level_name",
                "profile_id",
                "rating",
                "seller_id",
                "sold_items_count",
                "total_ratings",
                "truncated",
                "username",
                "won_auctions_count",
            ])
        );
        let badge = value
            .pointer("/badges/0")
            .unwrap_or_else(|| panic!("missing badge"));
        assert_eq!(
            keys(badge),
            BTreeSet::from(["category", "description", "name", "slug", "unlocked_at"])
        );
        assert!(badge.get("icon_url").is_none());
        assert_within_mcp_result_budget(&output);

        let schema = serde_json::to_value(schemars::schema_for!(SellerProfileOutput))
            .unwrap_or_else(|error| panic!("profile schema should serialize: {error}"));
        assert_eq!(
            schema.pointer("/properties/categories/items/maxLength"),
            Some(&serde_json::json!(512))
        );
    }

    #[test]
    fn profile_text_and_collections_are_bounded_with_an_explicit_signal() {
        let mut profile = profile_fixture();
        profile.bio = Some("💡".repeat(MAX_LONG_TEXT_BYTES));
        profile.categories = (0..=MAX_SELLER_CATEGORIES)
            .map(|index| format!("Category {index}"))
            .collect();
        let output = SellerProfileOutput::from(profile);
        assert!(output.truncated);
        assert_eq!(output.categories.len(), MAX_SELLER_CATEGORIES);
        assert!(
            output
                .bio
                .as_ref()
                .is_some_and(|bio| bio.len() <= MAX_LONG_TEXT_BYTES)
        );
    }

    #[test]
    fn rating_summary_shape_is_exact() {
        let output = serde_json::to_value(RatingSummaryOutput::from(RatingSummary {
            listing_id: 7,
            total_ratings: 12,
            average_rating: 4.5,
            rating_distribution: [0, 0, 1, 4, 7],
            total_comments: 3,
            has_ratings: true,
            has_comments: true,
            most_common_rating: Some(5),
            positive_percentage: 91.67,
        }))
        .unwrap_or_else(|error| panic!("rating should serialize: {error}"));
        assert_eq!(
            keys(&output),
            BTreeSet::from([
                "average_rating",
                "has_comments",
                "has_ratings",
                "listing_id",
                "most_common_rating",
                "positive_percentage",
                "rating_distribution",
                "total_comments",
                "total_ratings",
                "truncated",
            ])
        );
    }
}
