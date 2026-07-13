use budna_mcp_client::{
    CategoryPage, CategoryTranslations, CategoryWithFilters, FilterDefinition, FilterOption,
    FilterOptionList, FilterTranslations, FilterWithOptions, TranslationMap,
};
use rmcp::schemars;
use serde::Serialize;

use super::{
    MAX_CODE_BYTES, MAX_DISPLAY_TEXT_BYTES, MAX_NAME_BYTES, ProjectionBudget,
    UNTRUSTED_CONTENT_NOTICE,
    common::{FilterConfigurationOutput, PaginationOutput, ValidationRulesOutput},
};

const MAX_CATEGORY_RESULTS: usize = 200;
const MAX_FILTERS_PER_GROUP: usize = 75;
const MAX_FILTER_OPTIONS: usize = 100;

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CategoryFiltersOutput {
    pub content_notice: String,
    pub truncated: bool,
    pub category_id: i32,
    #[schemars(length(max = 256))]
    pub name: String,
    pub parent_id: Option<i32>,
    pub listing_count: i64,
    pub translations: Option<CategoryTranslationsOutput>,
    pub filters: CategoryFilterGroupsOutput,
}

impl From<CategoryWithFilters> for CategoryFiltersOutput {
    fn from(category: CategoryWithFilters) -> Self {
        let mut budget = ProjectionBudget::default();
        let name = budget.text(category.name, MAX_NAME_BYTES);
        let translations = category
            .translations
            .map(|translations| CategoryTranslationsOutput::project(translations, &mut budget));
        let filters = category
            .filters
            .map(|filters| CategoryFilterGroupsOutput::project(filters, &mut budget))
            .unwrap_or_default();
        let truncated = budget.truncated();

        Self {
            content_notice: UNTRUSTED_CONTENT_NOTICE.to_owned(),
            truncated,
            category_id: category.id,
            name,
            parent_id: category.parent_id,
            listing_count: category.listing_count,
            translations,
            filters,
        }
    }
}

#[derive(Debug, Default, Serialize, schemars::JsonSchema)]
pub struct CategoryFilterGroupsOutput {
    #[schemars(length(max = 75))]
    pub baseline_filters: Vec<FilterWithOptionsOutput>,
    #[schemars(length(max = 75))]
    pub category_filters: Vec<FilterWithOptionsOutput>,
    #[schemars(length(max = 75))]
    pub inherited_filters: Vec<FilterWithOptionsOutput>,
}

impl CategoryFilterGroupsOutput {
    fn project(filters: budna_mcp_client::CategoryFilters, budget: &mut ProjectionBudget) -> Self {
        Self {
            baseline_filters: project_filters(filters.baseline_filters, budget),
            category_filters: project_filters(filters.category_filters, budget),
            inherited_filters: project_filters(filters.inherited_filters, budget),
        }
    }
}

fn project_filters(
    filters: Vec<FilterWithOptions>,
    budget: &mut ProjectionBudget,
) -> Vec<FilterWithOptionsOutput> {
    budget
        .objects(filters, MAX_FILTERS_PER_GROUP)
        .into_iter()
        .map(|filter| FilterWithOptionsOutput::project(filter, budget))
        .collect()
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct FilterWithOptionsOutput {
    pub definition: FilterDefinitionOutput,
    #[schemars(length(max = 100))]
    pub options: Option<Vec<FilterOptionOutput>>,
}

impl FilterWithOptionsOutput {
    fn project(filter: FilterWithOptions, budget: &mut ProjectionBudget) -> Self {
        Self {
            definition: FilterDefinitionOutput::project(filter.definition, budget),
            options: filter
                .options
                .map(|options| project_filter_options(options, budget)),
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct FilterDefinitionOutput {
    pub id: i32,
    #[schemars(length(max = 128))]
    pub name: String,
    #[schemars(length(max = 256))]
    pub label: String,
    #[schemars(length(max = 128))]
    pub filter_type: String,
    pub is_baseline: bool,
    pub sortable: bool,
    pub configuration: FilterConfigurationOutput,
    pub validation_rules: ValidationRulesOutput,
    pub is_active: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub translations: Option<FilterTranslationsOutput>,
    pub option_count: Option<i32>,
}

impl FilterDefinitionOutput {
    fn project(definition: FilterDefinition, budget: &mut ProjectionBudget) -> Self {
        Self {
            id: definition.id,
            name: budget.text(definition.name, MAX_CODE_BYTES),
            label: budget.text(definition.label, MAX_NAME_BYTES),
            filter_type: budget.text(definition.filter_type, MAX_CODE_BYTES),
            is_baseline: definition.is_baseline,
            sortable: definition.sortable,
            configuration: FilterConfigurationOutput::project(definition.configuration, budget),
            validation_rules: ValidationRulesOutput::project(definition.validation_rules, budget),
            is_active: definition.is_active,
            created_at: definition.created_at,
            updated_at: definition.updated_at,
            translations: definition
                .translations
                .map(|translations| FilterTranslationsOutput::project(translations, budget)),
            option_count: definition.option_count,
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct FilterTranslationsOutput {
    pub label: Option<TranslationMapOutput>,
}

impl FilterTranslationsOutput {
    fn project(translations: FilterTranslations, budget: &mut ProjectionBudget) -> Self {
        Self {
            label: translations
                .label
                .map(|label| TranslationMapOutput::project(label, budget)),
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct FilterOptionOutput {
    pub id: i32,
    pub filter_id: i32,
    #[schemars(length(max = 512))]
    pub value: String,
    #[schemars(length(max = 512))]
    pub display_value: String,
    pub display_order: i32,
    pub is_active: bool,
    pub created_at: i64,
    pub is_suggested: bool,
    pub translations: Option<FilterOptionTranslationsOutput>,
}

impl FilterOptionOutput {
    fn project(option: FilterOption, budget: &mut ProjectionBudget) -> Self {
        Self {
            id: option.id,
            filter_id: option.filter_id,
            value: budget.text(option.value, MAX_DISPLAY_TEXT_BYTES),
            display_value: budget.text(option.display_value, MAX_DISPLAY_TEXT_BYTES),
            display_order: option.display_order,
            is_active: option.is_active,
            created_at: option.created_at,
            is_suggested: option.is_suggested,
            translations: option
                .translations
                .map(|translations| FilterOptionTranslationsOutput::project(translations, budget)),
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct FilterOptionTranslationsOutput {
    pub label: Option<TranslationMapOutput>,
}

impl FilterOptionTranslationsOutput {
    fn project(
        translations: budna_mcp_client::FilterOptionTranslations,
        budget: &mut ProjectionBudget,
    ) -> Self {
        Self {
            label: translations
                .label
                .map(|label| TranslationMapOutput::project(label, budget)),
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct FilterOptionsOutput {
    pub content_notice: String,
    pub truncated: bool,
    pub filter_id: i32,
    pub total: u32,
    #[schemars(length(max = 100))]
    pub options: Vec<FilterOptionOutput>,
}

impl From<FilterOptionList> for FilterOptionsOutput {
    fn from(list: FilterOptionList) -> Self {
        let mut budget = ProjectionBudget::default();
        let options = project_filter_options(list.options, &mut budget);
        let truncated = budget.truncated();
        Self {
            content_notice: UNTRUSTED_CONTENT_NOTICE.to_owned(),
            truncated,
            filter_id: list.filter_id,
            total: list.total,
            options,
        }
    }
}

fn project_filter_options(
    options: Vec<FilterOption>,
    budget: &mut ProjectionBudget,
) -> Vec<FilterOptionOutput> {
    budget
        .objects(options, MAX_FILTER_OPTIONS)
        .into_iter()
        .map(|option| FilterOptionOutput::project(option, budget))
        .collect()
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CategoryListOutput {
    pub content_notice: String,
    pub truncated: bool,
    #[schemars(length(max = 200))]
    pub categories: Vec<CategoryOutput>,
    pub pagination: PaginationOutput,
}

impl From<CategoryPage> for CategoryListOutput {
    fn from(page: CategoryPage) -> Self {
        let mut budget = ProjectionBudget::default();
        let categories = budget
            .objects(page.items, MAX_CATEGORY_RESULTS)
            .into_iter()
            .map(|category| CategoryOutput::project(category, &mut budget))
            .collect();
        let truncated = budget.truncated();
        Self {
            content_notice: UNTRUSTED_CONTENT_NOTICE.to_owned(),
            truncated,
            categories,
            pagination: PaginationOutput::from(page.pagination),
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CategoryOutput {
    pub id: i32,
    #[schemars(length(max = 256))]
    pub name: String,
    pub parent_id: Option<i32>,
    pub listing_count: i64,
    pub translations: Option<CategoryTranslationsOutput>,
}

impl CategoryOutput {
    fn project(category: budna_mcp_client::CategorySummary, budget: &mut ProjectionBudget) -> Self {
        Self {
            id: category.id,
            name: budget.text(category.name, MAX_NAME_BYTES),
            parent_id: category.parent_id,
            listing_count: category.listing_count,
            translations: category
                .translations
                .map(|translations| CategoryTranslationsOutput::project(translations, budget)),
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CategoryTranslationsOutput {
    pub name: TranslationMapOutput,
}

impl CategoryTranslationsOutput {
    fn project(translations: CategoryTranslations, budget: &mut ProjectionBudget) -> Self {
        Self {
            name: TranslationMapOutput::project(translations.name, budget),
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TranslationMapOutput {
    #[schemars(length(max = 256))]
    pub en: String,
    #[schemars(length(max = 256))]
    pub sv: String,
    #[schemars(length(max = 256))]
    pub no: String,
}

impl TranslationMapOutput {
    fn project(translations: TranslationMap, budget: &mut ProjectionBudget) -> Self {
        Self {
            en: budget.text(translations.en, MAX_NAME_BYTES),
            sv: budget.text(translations.sv, MAX_NAME_BYTES),
            no: budget.text(translations.no, MAX_NAME_BYTES),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use budna_mcp_client::{
        CategoryFilters, CategorySummary, FilterConfiguration, FilterOptionTranslations,
        Pagination, ValidationRules,
    };

    use super::*;
    use crate::output::assert_within_mcp_result_budget;

    fn keys(value: &serde_json::Value) -> BTreeSet<&str> {
        value
            .as_object()
            .map(|object| object.keys().map(String::as_str).collect())
            .unwrap_or_else(|| panic!("expected an object"))
    }

    fn translations() -> TranslationMap {
        TranslationMap {
            en: "Mount".to_owned(),
            sv: "Fattning".to_owned(),
            no: "Fatning".to_owned(),
        }
    }

    fn definition(id: i32) -> FilterDefinition {
        FilterDefinition {
            id,
            name: "mount".to_owned(),
            label: "Mount".to_owned(),
            filter_type: "select".to_owned(),
            is_baseline: false,
            sortable: true,
            configuration: FilterConfiguration {
                placeholder: Some("Choose mount".to_owned()),
                min_value: None,
                max_value: None,
                step: None,
                unit: None,
                max_stars: None,
                multiple: Some(false),
                required: Some(false),
                searchable: Some(true),
            },
            validation_rules: ValidationRules {
                min_length: None,
                max_length: None,
                pattern: Some("^[a-z-]+$".to_owned()),
                required: Some(false),
            },
            is_active: true,
            created_at: 1,
            updated_at: 2,
            translations: Some(FilterTranslations {
                label: Some(translations()),
            }),
            option_count: Some(1),
        }
    }

    fn option(id: i32, filter_id: i32) -> FilterOption {
        FilterOption {
            id,
            filter_id,
            value: "sony-e".to_owned(),
            display_value: "Sony E".to_owned(),
            display_order: 1,
            is_active: true,
            created_at: 1,
            is_suggested: true,
            translations: Some(FilterOptionTranslations {
                label: Some(translations()),
            }),
        }
    }

    #[test]
    fn category_list_has_notice_truncation_and_exact_nested_shapes() {
        let output = CategoryListOutput::from(CategoryPage {
            items: vec![CategorySummary {
                id: 12,
                name: "Cameras".to_owned(),
                parent_id: None,
                listing_count: 4,
                translations: Some(CategoryTranslations {
                    name: translations(),
                }),
            }],
            pagination: Pagination {
                page: 1,
                limit: 100,
                total: 1,
                total_pages: 1,
            },
        });
        let value = serde_json::to_value(&output)
            .unwrap_or_else(|error| panic!("category list should serialize: {error}"));

        assert_eq!(
            keys(&value),
            BTreeSet::from(["categories", "content_notice", "pagination", "truncated",])
        );
        assert_eq!(
            keys(
                value
                    .pointer("/categories/0")
                    .unwrap_or_else(|| panic!("missing category"))
            ),
            BTreeSet::from(["id", "listing_count", "name", "parent_id", "translations"])
        );
        assert_eq!(
            keys(
                value
                    .pointer("/categories/0/translations/name")
                    .unwrap_or_else(|| panic!("missing translations"))
            ),
            BTreeSet::from(["en", "no", "sv"])
        );
        assert_eq!(
            keys(
                value
                    .pointer("/categories/0/translations")
                    .unwrap_or_else(|| panic!("missing category translations"))
            ),
            BTreeSet::from(["name"])
        );
        assert_within_mcp_result_budget(&output);
    }

    #[test]
    fn category_filter_and_option_outputs_have_complete_allowlists() {
        let filter = FilterWithOptions {
            definition: definition(277),
            options: Some(vec![option(1, 277)]),
        };
        let output = CategoryFiltersOutput::from(CategoryWithFilters {
            id: 12,
            name: "Cameras".to_owned(),
            parent_id: None,
            listing_count: 4,
            filters: Some(CategoryFilters {
                baseline_filters: Vec::new(),
                category_filters: vec![filter],
                inherited_filters: Vec::new(),
            }),
            translations: Some(CategoryTranslations {
                name: translations(),
            }),
        });
        let value = serde_json::to_value(&output)
            .unwrap_or_else(|error| panic!("category filters should serialize: {error}"));
        assert_eq!(
            keys(&value),
            BTreeSet::from([
                "category_id",
                "content_notice",
                "filters",
                "listing_count",
                "name",
                "parent_id",
                "translations",
                "truncated",
            ])
        );
        let filter = value
            .pointer("/filters/category_filters/0")
            .unwrap_or_else(|| panic!("missing filter"));
        assert_eq!(
            keys(
                value
                    .pointer("/filters")
                    .unwrap_or_else(|| panic!("missing filter groups"))
            ),
            BTreeSet::from(["baseline_filters", "category_filters", "inherited_filters",])
        );
        assert_eq!(keys(filter), BTreeSet::from(["definition", "options"]));
        assert_eq!(
            keys(
                filter
                    .pointer("/definition")
                    .unwrap_or_else(|| panic!("missing definition"))
            ),
            BTreeSet::from([
                "configuration",
                "created_at",
                "filter_type",
                "id",
                "is_active",
                "is_baseline",
                "label",
                "name",
                "option_count",
                "sortable",
                "translations",
                "updated_at",
                "validation_rules",
            ])
        );
        assert_eq!(
            keys(
                filter
                    .pointer("/definition/configuration")
                    .unwrap_or_else(|| panic!("missing configuration"))
            ),
            BTreeSet::from([
                "max_stars",
                "max_value",
                "min_value",
                "multiple",
                "placeholder",
                "required",
                "searchable",
                "step",
                "unit",
            ])
        );
        assert_eq!(
            keys(
                filter
                    .pointer("/definition/validation_rules")
                    .unwrap_or_else(|| panic!("missing validation"))
            ),
            BTreeSet::from(["max_length", "min_length", "pattern", "required"])
        );
        assert_eq!(
            keys(
                filter
                    .pointer("/definition/translations")
                    .unwrap_or_else(|| panic!("missing filter translations"))
            ),
            BTreeSet::from(["label"])
        );
        assert_eq!(
            keys(
                filter
                    .pointer("/definition/translations/label")
                    .unwrap_or_else(|| panic!("missing translated filter label"))
            ),
            BTreeSet::from(["en", "no", "sv"])
        );
        assert_eq!(
            keys(
                filter
                    .pointer("/options/0")
                    .unwrap_or_else(|| panic!("missing option"))
            ),
            BTreeSet::from([
                "created_at",
                "display_order",
                "display_value",
                "filter_id",
                "id",
                "is_active",
                "is_suggested",
                "translations",
                "value",
            ])
        );
        assert_eq!(
            keys(
                filter
                    .pointer("/options/0/translations")
                    .unwrap_or_else(|| panic!("missing option translations"))
            ),
            BTreeSet::from(["label"])
        );

        let options = FilterOptionsOutput::from(FilterOptionList {
            options: vec![option(1, 277)],
            filter_id: 277,
            total: 1,
        });
        let options = serde_json::to_value(options)
            .unwrap_or_else(|error| panic!("options should serialize: {error}"));
        assert_eq!(
            keys(&options),
            BTreeSet::from([
                "content_notice",
                "filter_id",
                "options",
                "total",
                "truncated",
            ])
        );
    }

    #[test]
    fn aggregate_object_and_text_budgets_bound_filter_output() {
        let oversized_options = (0..MAX_FILTER_OPTIONS + 1)
            .map(|id| {
                let mut option = option(id as i32, 277);
                option.display_value = "\u{0000}".repeat(MAX_DISPLAY_TEXT_BYTES);
                option
            })
            .collect();
        let output = FilterOptionsOutput::from(FilterOptionList {
            options: oversized_options,
            filter_id: 277,
            total: (MAX_FILTER_OPTIONS + 1) as u32,
        });
        assert!(output.truncated);
        assert_eq!(output.options.len(), MAX_FILTER_OPTIONS);
        assert_within_mcp_result_budget(&output);
    }

    #[test]
    fn maximum_category_filter_shape_stays_within_the_total_output_budget() {
        let filters = (0..MAX_FILTERS_PER_GROUP)
            .map(|filter_index| FilterWithOptions {
                definition: definition(filter_index as i32 + 1),
                options: Some(
                    (0..MAX_FILTER_OPTIONS)
                        .map(|option_index| {
                            option(
                                (filter_index * MAX_FILTER_OPTIONS + option_index) as i32 + 1,
                                filter_index as i32 + 1,
                            )
                        })
                        .collect(),
                ),
            })
            .collect();
        let output = CategoryFiltersOutput::from(CategoryWithFilters {
            id: 12,
            name: "Cameras".to_owned(),
            parent_id: None,
            listing_count: 4,
            filters: Some(CategoryFilters {
                baseline_filters: filters,
                category_filters: Vec::new(),
                inherited_filters: Vec::new(),
            }),
            translations: None,
        });

        assert!(output.truncated);
        assert_within_mcp_result_budget(&output);
    }
}
