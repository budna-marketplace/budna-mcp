use budna_mcp_client::{FacetCount, Money, Pagination};
use rmcp::schemars;
use serde::Serialize;

use super::{MAX_CODE_BYTES, MAX_DISPLAY_TEXT_BYTES, ProjectionBudget};

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct MoneyOutput {
    #[schemars(length(max = 128))]
    pub amount: String,
    #[schemars(length(min = 3, max = 3), regex(pattern = "^[A-Z]{3}$"))]
    pub currency_code: String,
}

impl From<Money> for MoneyOutput {
    fn from(money: Money) -> Self {
        Self {
            amount: money.amount,
            currency_code: money.currency_code,
        }
    }
}

impl MoneyOutput {
    pub(super) fn from_amount(amount: String, currency_code: &str) -> Self {
        Self {
            amount,
            currency_code: currency_code.to_owned(),
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct PaginationOutput {
    pub page: i64,
    pub limit: i64,
    pub total: i64,
    pub total_pages: i64,
}

impl From<Pagination> for PaginationOutput {
    fn from(pagination: Pagination) -> Self {
        Self {
            page: pagination.page,
            limit: pagination.limit,
            total: pagination.total,
            total_pages: pagination.total_pages,
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct FacetCountOutput {
    #[schemars(length(max = 512))]
    pub value: String,
    pub count: u64,
}

impl FacetCountOutput {
    pub(super) fn project(bucket: FacetCount, budget: &mut ProjectionBudget) -> Self {
        Self {
            value: budget.text(bucket.value, MAX_DISPLAY_TEXT_BYTES),
            count: bucket.count,
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct FilterConfigurationOutput {
    #[schemars(length(max = 512))]
    pub placeholder: Option<String>,
    #[schemars(length(max = 128))]
    pub min_value: Option<String>,
    #[schemars(length(max = 128))]
    pub max_value: Option<String>,
    #[schemars(length(max = 128))]
    pub step: Option<String>,
    #[schemars(length(max = 128))]
    pub unit: Option<String>,
    pub max_stars: Option<i32>,
    pub multiple: Option<bool>,
    pub required: Option<bool>,
    pub searchable: Option<bool>,
}

impl FilterConfigurationOutput {
    pub(super) fn project(
        configuration: budna_mcp_client::FilterConfiguration,
        budget: &mut ProjectionBudget,
    ) -> Self {
        Self {
            placeholder: budget.optional_text(configuration.placeholder, MAX_DISPLAY_TEXT_BYTES),
            min_value: budget.optional_text(configuration.min_value, MAX_CODE_BYTES),
            max_value: budget.optional_text(configuration.max_value, MAX_CODE_BYTES),
            step: budget.optional_text(configuration.step, MAX_CODE_BYTES),
            unit: budget.optional_text(configuration.unit, MAX_CODE_BYTES),
            max_stars: configuration.max_stars,
            multiple: configuration.multiple,
            required: configuration.required,
            searchable: configuration.searchable,
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ValidationRulesOutput {
    pub min_length: Option<i32>,
    pub max_length: Option<i32>,
    #[schemars(length(max = 512))]
    pub pattern: Option<String>,
    pub required: Option<bool>,
}

impl ValidationRulesOutput {
    pub(super) fn project(
        rules: budna_mcp_client::ValidationRules,
        budget: &mut ProjectionBudget,
    ) -> Self {
        Self {
            min_length: rules.min_length,
            max_length: rules.max_length,
            pattern: budget.optional_text(rules.pattern, MAX_DISPLAY_TEXT_BYTES),
            required: rules.required,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::json;

    use super::*;

    fn keys(value: &serde_json::Value) -> BTreeSet<&str> {
        value
            .as_object()
            .map(|object| object.keys().map(String::as_str).collect())
            .unwrap_or_else(|| panic!("expected an object"))
    }

    #[test]
    fn common_projection_shapes_are_server_owned_and_exact() {
        let money = serde_json::to_value(MoneyOutput::from(Money {
            amount: "12.30".to_owned(),
            currency_code: "NOK".to_owned(),
        }))
        .unwrap_or_else(|error| panic!("money should serialize: {error}"));
        assert_eq!(keys(&money), BTreeSet::from(["amount", "currency_code"]));
        assert_eq!(money, json!({"amount": "12.30", "currency_code": "NOK"}));

        let pagination = serde_json::to_value(PaginationOutput::from(Pagination {
            page: 1,
            limit: 10,
            total: 23,
            total_pages: 3,
        }))
        .unwrap_or_else(|error| panic!("pagination should serialize: {error}"));
        assert_eq!(
            keys(&pagination),
            BTreeSet::from(["limit", "page", "total", "total_pages"])
        );
    }
}
