use std::{
    collections::{BTreeMap, BTreeSet},
    process::Stdio,
    time::Duration,
};

use anyhow::{Context, Result};
use rmcp::{
    ServiceExt,
    model::{
        CallToolRequestParams, ClientCapabilities, ClientInfo, ExtensionCapabilities,
        Implementation, ReadResourceRequestParams, ResourceContents,
    },
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use serde_json::json;
use tokio::io::AsyncReadExt;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

const PROTOCOL_TIMEOUT: Duration = Duration::from_secs(10);
const APP_EXTENSION_ID: &str = "io.modelcontextprotocol/ui";
const APP_RESOURCE_URI: &str = "ui://budna/marketplace-explorer-v1.html";
const APP_MIME_TYPE: &str = "text/html;profile=mcp-app";
const CONFIGURED_IMAGE_ORIGIN: &str = "https://images.example.test";
const CONFIGURED_LISTING_ORIGIN: &str = "https://listings.example.test";
const PRIVATE_SENTINEL: &str = "private-contract-sentinel";

fn apps_client_info() -> Result<ClientInfo> {
    let settings = json!({"mimeTypes": [APP_MIME_TYPE]})
        .as_object()
        .cloned()
        .context("MCP Apps capability should be an object")?;
    let mut extensions = ExtensionCapabilities::new();
    extensions.insert(APP_EXTENSION_ID.to_owned(), settings);

    Ok(ClientInfo::new(
        ClientCapabilities::builder()
            .enable_extensions_with(extensions)
            .build(),
        Implementation::new("budna-mcp-protocol-test", "1.0.0"),
    ))
}

fn error_payload(result: &rmcp::model::CallToolResult) -> Result<serde_json::Value> {
    let content = result
        .content
        .first()
        .and_then(rmcp::model::ContentBlock::as_text)
        .context("tool error should contain text content")?;
    let payload: serde_json::Value =
        serde_json::from_str(&content.text).context("tool error content should be JSON")?;
    assert_eq!(
        result.structured_content.as_ref(),
        Some(&payload),
        "tool error text and structured representations must agree"
    );
    Ok(payload)
}

fn listing_data(
    listing_id: i64,
    seller_id: i64,
    approved: bool,
    status: &str,
) -> serde_json::Value {
    json!({
        "id": listing_id,
        "seller_id": seller_id,
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
        "bid_count": 2,
        "approved": approved,
        "featured": false,
        "tags": ["camera"],
        "image_ids": ["123e4567-e89b-12d3-a456-426614174000"],
        "created_at": 1_700_000_000_000_i64,
        "updated_at": 1_700_000_000_100_i64,
        "package_size": "medium",
        "package_weight_grams": 900,
        "shipping_provider_codes": ["postnord"],
        "location": {"city": "Oslo", "region": "Oslo", "country": "NO"},
        "allow_pickup": true,
        "buyer_protection_config": {
            "rate_percent": "5.0",
            "flat_fee": "10.00",
            "mandatory_threshold": "100.00",
            "cap": "500.00",
            "enabled": true
        },
        "server_only_marker": PRIVATE_SENTINEL
    })
}

fn listing_envelope(listing_id: i64) -> serde_json::Value {
    json!({
        "success": true,
        "data": listing_data(listing_id, 42, true, "active"),
        "server_only_marker": PRIVATE_SENTINEL
    })
}

fn listing_page_envelope(
    listing_id: i64,
    seller_id: i64,
    page: i64,
    limit: i64,
) -> serde_json::Value {
    let total = (page - 1) * limit + 1;
    json!({
        "success": true,
        "data": {
            "items": [listing_data(listing_id, seller_id, true, "active")],
            "pagination": {"page": page, "limit": limit, "total": total, "total_pages": page},
            "server_only_marker": PRIVATE_SENTINEL
        }
    })
}

async fn mount_public_tool_mocks(api: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v1/categories"))
        .and(query_param("page", "2"))
        .and(query_param("limit", "5"))
        .and(query_param("parent_id", "2"))
        .and(query_param("include_filters", "false"))
        .and(query_param("translations", "false"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "data": {
                "items": [{
                    "id": 12,
                    "name": "Cameras",
                    "parent_id": 2,
                    "created_at": 1_700_000_000_000_i64,
                    "listing_count": 4,
                    "filters": null,
                    "translations": null,
                    "server_only_marker": PRIVATE_SENTINEL
                }],
                "pagination": {"page": 2, "limit": 5, "total": 6, "total_pages": 2}
            }
        })))
        .expect(1)
        .mount(api)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v1/search/listings"))
        .and(query_param("q", "camera"))
        .and(query_param("category_id", "12"))
        .and(query_param("market", "norwegian"))
        .and(query_param("min_price", "100"))
        .and(query_param("max_price", "200"))
        .and(query_param("condition", "good"))
        .and(query_param("listing_type", "auction"))
        .and(query_param("status", "active"))
        .and(query_param("ending_soon", "true"))
        .and(query_param("featured", "false"))
        .and(query_param("free_shipping", "false"))
        .and(query_param("sort_by", "price"))
        .and(query_param("sort_order", "asc"))
        .and(query_param("page", "2"))
        .and(query_param("per_page", "7"))
        .and(query_param("include_facets", "true"))
        .and(query_param("search_mode", "keyword"))
        .and(query_param("location_id", "3"))
        .and(query_param("location_region", "Oslo"))
        .and(query_param("location_municipality", "Oslo"))
        .and(query_param("allow_pickup", "true"))
        .and(query_param("attr_mount", "sony-e"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "data": {
                "hits": [{
                    "id": 7,
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
                    "status": "active",
                    "start_time": 1_700_000_000_000_i64,
                    "end_time": 1_800_000_000_000_i64,
                    "featured": false,
                    "tags": ["camera"],
                    "image_ids": ["123e4567-e89b-12d3-a456-426614174000"],
                    "primary_image_id": "123e4567-e89b-12d3-a456-426614174000",
                    "ending_soon": false,
                    "has_bids": true,
                    "server_only_marker": PRIVATE_SENTINEL
                }],
                "total": 8,
                "page": 2,
                "per_page": 7,
                "total_pages": 2,
                "search_time_ms": 3,
                "facets": {
                    "categories": [{"value": "12", "count": 1}],
                    "conditions": [],
                    "listing_types": [],
                    "markets": [],
                    "statuses": [],
                    "regions": [],
                    "cities": [],
                    "allow_pickup": [],
                    "price_stats": null
                },
                "server_only_marker": PRIVATE_SENTINEL
            }
        })))
        .expect(1)
        .mount(api)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v1/listings/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(listing_envelope(7)))
        .expect(5)
        .mount(api)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v1/listings/7/attributes"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "data": {
                "listing_id": 7,
                "attributes": [{
                    "id": 1,
                    "listing_id": 7,
                    "filter_definition_id": 277,
                    "filter_name": "mount",
                    "label": "Mount",
                    "value": {"type": "Json", "value": {"secret": PRIVATE_SENTINEL}},
                    "display_value": "Sony E",
                    "created_at": 1_700_000_000_000_i64,
                    "updated_at": 1_700_000_000_100_i64,
                    "server_only_marker": PRIVATE_SENTINEL
                }]
            }
        })))
        .expect(1)
        .mount(api)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v1/listings/7/related"))
        .and(query_param("page", "2"))
        .and(query_param("limit", "3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(listing_page_envelope(8, 84, 2, 3)))
        .expect(1)
        .mount(api)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v1/listings/seller/42"))
        .and(query_param("page", "3"))
        .and(query_param("limit", "4"))
        .respond_with(ResponseTemplate::new(200).set_body_json(listing_page_envelope(9, 42, 3, 4)))
        .expect(1)
        .mount(api)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v1/categories/12"))
        .and(query_param("include_filters", "true"))
        .and(query_param("translations", "false"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "data": {
                "id": 12,
                "name": "Cameras",
                "parent_id": 2,
                "listing_count": 4,
                "filters": {
                    "baseline_filters": [],
                    "category_filters": [],
                    "inherited_filters": []
                },
                "translations": null,
                "server_only_marker": PRIVATE_SENTINEL
            }
        })))
        .expect(1)
        .mount(api)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v1/filters/277/options"))
        .and(query_param("translations", "false"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "data": {
                "filter_id": 277,
                "total": 1,
                "options": [{
                    "id": 1,
                    "filter_id": 277,
                    "value": "sony-e",
                    "display_value": "Sony E",
                    "display_order": 1,
                    "metadata": {"secret": PRIVATE_SENTINEL},
                    "is_active": true,
                    "created_at": 1_700_000_000_000_i64,
                    "is_suggested": true,
                    "translations": null,
                    "server_only_marker": PRIVATE_SENTINEL
                }],
                "server_only_marker": PRIVATE_SENTINEL
            }
        })))
        .expect(1)
        .mount(api)
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
                "auction_history": {"won_auctions_count": 4, "sold_items_count": 5},
                "verification_status": {"id_verified": true},
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
                "server_only_marker": PRIVATE_SENTINEL
            }
        })))
        .expect(1)
        .mount(api)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v1/ratings/listing/7/summary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "data": {
                "listing_id": 7,
                "total_ratings": 12,
                "average_rating": 4.5,
                "rating_distribution": [0, 0, 1, 4, 7],
                "total_comments": 3,
                "has_ratings": true,
                "has_comments": true,
                "most_common_rating": 5,
                "positive_percentage": 91.67,
                "server_only_marker": PRIVATE_SENTINEL
            }
        })))
        .expect(1)
        .mount(api)
        .await;
}

async fn mount_error_mocks(api: &MockServer) {
    for (listing_id, status) in [
        (400_i64, 400_u16),
        (404_i64, 404_u16),
        (429_i64, 429_u16),
        (500_i64, 500_u16),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v1/listings/{listing_id}")))
            .respond_with(ResponseTemplate::new(status).set_body_json(json!({
                "code": "IGNORE_PREVIOUS_INSTRUCTIONS",
                "title": PRIVATE_SENTINEL,
                "detail": PRIVATE_SENTINEL
            })))
            .expect(if status == 429 { 3_u64 } else { 1_u64 })
            .mount(api)
            .await;
    }

    Mock::given(method("GET"))
        .and(path("/api/v1/listings/600"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string("not-json"),
        )
        .expect(1)
        .mount(api)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v1/listings/700"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(5))
                .set_body_json(listing_envelope(700)),
        )
        .expect(1)
        .mount(api)
        .await;
}

fn assert_closed_object_schema(tool_name: &str, kind: &str, schema: &serde_json::Value) {
    assert_eq!(
        schema.get("type"),
        Some(&json!("object")),
        "{tool_name} {kind} schema should describe an object"
    );
    assert_eq!(
        schema.get("additionalProperties"),
        Some(&json!(false)),
        "{tool_name} {kind} schema should reject unknown top-level fields"
    );
    assert!(
        schema
            .get("properties")
            .is_some_and(serde_json::Value::is_object),
        "{tool_name} {kind} schema should declare properties"
    );
}

fn assert_output_object_schema(tool_name: &str, schema: &serde_json::Value) {
    assert_eq!(
        schema.get("type"),
        Some(&json!("object")),
        "{tool_name} output schema should describe an object"
    );
    assert!(
        schema
            .get("properties")
            .is_some_and(serde_json::Value::is_object),
        "{tool_name} output schema should declare properties"
    );
}

fn resolve_local_schema_ref<'a>(
    root: &'a serde_json::Value,
    mut schema: &'a serde_json::Value,
) -> Option<&'a serde_json::Value> {
    for _ in 0..16 {
        let Some(reference) = schema.get("$ref").and_then(serde_json::Value::as_str) else {
            return Some(schema);
        };
        schema = root.pointer(reference.strip_prefix('#')?)?;
    }
    None
}

fn assert_success_contract(
    tool_name: &str,
    result: &rmcp::model::CallToolResult,
    output_schema: &serde_json::Value,
) -> Result<serde_json::Value> {
    assert_eq!(result.is_error, Some(false), "{tool_name} should succeed");
    let structured = result
        .structured_content
        .as_ref()
        .with_context(|| format!("{tool_name} should return structured content"))?;
    let text = result
        .content
        .first()
        .and_then(rmcp::model::ContentBlock::as_text)
        .with_context(|| format!("{tool_name} should return a text fallback"))?;
    let text_json: serde_json::Value = serde_json::from_str(&text.text)
        .with_context(|| format!("{tool_name} text fallback should be JSON"))?;
    assert_eq!(
        &text_json, structured,
        "{tool_name} text and structured representations must agree"
    );

    let output = structured
        .as_object()
        .with_context(|| format!("{tool_name} output should be an object"))?;
    let properties = output_schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .with_context(|| format!("{tool_name} output schema should declare properties"))?;
    assert_eq!(
        output.keys().collect::<BTreeSet<_>>(),
        properties.keys().collect::<BTreeSet<_>>(),
        "{tool_name} should return its complete advertised root shape"
    );
    for required in output_schema
        .get("required")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
    {
        assert!(
            output.contains_key(required),
            "{tool_name} should return required field {required}"
        );
    }

    let encoded = serde_json::to_string(structured)?;
    assert!(
        !encoded.contains(PRIVATE_SENTINEL),
        "{tool_name} must not expose ignored private response fields or raw JSON values"
    );
    Ok(structured.clone())
}

fn assert_tool_error(
    result: &rmcp::model::CallToolResult,
    expected_operation: &str,
    expected_code: &str,
    expected_status: Option<u16>,
    expected_retryable: bool,
) -> Result<()> {
    assert_eq!(result.is_error, Some(true));
    let payload = error_payload(result)?;
    assert_eq!(payload.get("operation"), Some(&json!(expected_operation)));
    assert_eq!(payload.pointer("/error/code"), Some(&json!(expected_code)));
    assert_eq!(
        payload.pointer("/error/http_status"),
        Some(&expected_status.map_or(serde_json::Value::Null, serde_json::Value::from))
    );
    assert_eq!(
        payload.pointer("/error/retryable"),
        Some(&json!(expected_retryable))
    );
    assert!(
        payload
            .pointer("/error/message")
            .is_some_and(serde_json::Value::is_string)
    );
    assert!(!payload.to_string().contains(PRIVATE_SENTINEL));
    Ok(())
}

#[tokio::test]
async fn packaged_binary_serves_the_public_explore_profile_over_stdio() -> Result<()> {
    let api = MockServer::start().await;
    mount_public_tool_mocks(&api).await;
    mount_error_mocks(&api).await;

    let command = tokio::process::Command::new(env!("CARGO_BIN_EXE_budna-mcp")).configure(|cmd| {
        cmd.arg("--api-url")
            .arg(api.uri())
            .arg("--request-timeout-secs")
            .arg("2")
            .env("BUDNA_PUBLIC_LISTING_ORIGIN", CONFIGURED_LISTING_ORIGIN)
            .env("BUDNA_IMAGE_ORIGIN", CONFIGURED_IMAGE_ORIGIN)
            .env("BUDNA_MCP_LOG", "trace");
    });
    let (transport, stderr) = TokioChildProcess::builder(command)
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to start the packaged budna-mcp binary")?;
    let mut stderr = stderr.context("child stderr should be captured")?;

    let mut client = tokio::time::timeout(PROTOCOL_TIMEOUT, apps_client_info()?.serve(transport))
        .await
        .context("MCP initialization timed out")??;

    let server_info = client
        .peer_info()
        .context("MCP initialization should return server information")?;
    assert_eq!(server_info.server_info.name, "budna-mcp");
    assert!(server_info.capabilities.tools.is_some());
    assert!(server_info.capabilities.resources.is_some());
    assert!(
        server_info
            .capabilities
            .extensions
            .as_ref()
            .is_some_and(|extensions| extensions.contains_key(APP_EXTENSION_ID))
    );
    assert!(
        server_info
            .instructions
            .as_deref()
            .is_some_and(|value| value.contains("public Explore"))
    );

    let tools = tokio::time::timeout(PROTOCOL_TIMEOUT, client.list_all_tools())
        .await
        .context("tools/list timed out")??;
    let tool_names = tools
        .iter()
        .map(|tool| tool.name.as_ref())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        tool_names,
        BTreeSet::from([
            "get_categories",
            "get_category_filters",
            "get_filter_options",
            "get_listing",
            "get_listing_attributes",
            "get_listing_bid_summary",
            "get_listing_related",
            "get_public_seller_profile",
            "get_public_ratings_summary",
            "get_seller_listings",
            "search_listings",
        ])
    );

    let app_tool_names = BTreeSet::from([
        "get_listing",
        "get_listing_related",
        "get_seller_listings",
        "search_listings",
    ]);
    let mut output_schemas = BTreeMap::new();
    for tool in &tools {
        let input_schema = serde_json::Value::Object((*tool.input_schema).clone());
        assert_closed_object_schema(tool.name.as_ref(), "input", &input_schema);
        if tool.name == "search_listings" {
            assert_eq!(
                input_schema.pointer("/properties/custom_filters/maxProperties"),
                Some(&json!(20))
            );
            assert_eq!(
                input_schema.pointer("/properties/custom_filters/propertyNames/pattern"),
                Some(&json!("^attr_[A-Za-z0-9_-]{1,96}$"))
            );
            assert_eq!(
                input_schema.pointer("/properties/custom_filters/additionalProperties/type"),
                Some(&json!("string"))
            );
            assert_eq!(
                input_schema.pointer("/properties/custom_filters/additionalProperties/minLength"),
                Some(&json!(1))
            );
            assert_eq!(
                input_schema.pointer("/properties/custom_filters/additionalProperties/maxLength"),
                Some(&json!(200))
            );
            assert_eq!(
                input_schema.pointer("/properties/sort_by/pattern"),
                Some(&json!(
                    "^(?:relevance|price|created_at|end_time|popularity|attr_[A-Za-z0-9_-]{1,45})$"
                ))
            );
        }
        let output_schema = tool
            .output_schema
            .as_ref()
            .with_context(|| format!("{} should advertise an output schema", tool.name))?;
        let output_schema = serde_json::Value::Object((**output_schema).clone());
        assert_output_object_schema(tool.name.as_ref(), &output_schema);
        output_schemas.insert(tool.name.to_string(), output_schema);

        let annotations = tool
            .annotations
            .as_ref()
            .with_context(|| format!("{} should advertise annotations", tool.name))?;
        assert_eq!(annotations.read_only_hint, Some(true));
        assert_eq!(annotations.destructive_hint, Some(false));
        assert_eq!(annotations.idempotent_hint, Some(true));
        assert_eq!(annotations.open_world_hint, Some(true));

        let metadata = tool
            .meta
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .with_context(|| format!("{} metadata should serialize", tool.name))?;
        if app_tool_names.contains(tool.name.as_ref()) {
            let metadata = metadata
                .as_ref()
                .with_context(|| format!("{} should advertise MCP App metadata", tool.name))?;
            assert_eq!(
                metadata.pointer("/ui/resourceUri"),
                Some(&json!(APP_RESOURCE_URI))
            );
            assert_eq!(
                metadata.pointer("/ui/visibility"),
                Some(&json!(["model", "app"]))
            );
            assert_eq!(
                metadata.get("ui/resourceUri"),
                Some(&json!(APP_RESOURCE_URI))
            );
        } else {
            assert!(metadata.is_none());
        }
    }

    let search_schema = output_schemas
        .get("search_listings")
        .context("search output schema should be retained")?;
    let hits_schema = resolve_local_schema_ref(
        search_schema,
        search_schema
            .pointer("/properties/hits")
            .context("search output should declare hits")?,
    )
    .context("hits schema should resolve locally")?;
    let card_schema = resolve_local_schema_ref(
        search_schema,
        hits_schema
            .get("items")
            .context("hits schema should declare item constraints")?,
    )
    .context("listing card schema should resolve locally")?;
    assert_eq!(
        card_schema.pointer("/properties/tags/items/maxLength"),
        Some(&json!(512))
    );
    assert_eq!(
        card_schema.pointer("/properties/image_ids/items/pattern"),
        Some(&json!(
            "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
        ))
    );

    let detail_schema = output_schemas
        .get("get_listing")
        .context("listing detail output schema should be retained")?;
    assert_eq!(
        detail_schema.pointer("/properties/shipping_provider_codes/items/maxLength"),
        Some(&json!(128))
    );
    let profile_schema = output_schemas
        .get("get_public_seller_profile")
        .context("profile output schema should be retained")?;
    assert_eq!(
        profile_schema.pointer("/properties/categories/items/maxLength"),
        Some(&json!(512))
    );

    let resources = tokio::time::timeout(PROTOCOL_TIMEOUT, client.list_all_resources())
        .await
        .context("resources/list timed out")??;
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].uri, APP_RESOURCE_URI);
    assert_eq!(resources[0].mime_type.as_deref(), Some(APP_MIME_TYPE));
    let resource_metadata = serde_json::to_value(resources[0].meta.as_ref())
        .context("resource metadata should serialize")?;
    assert_eq!(
        resource_metadata.pointer("/ui/csp/resourceDomains"),
        Some(&json!([CONFIGURED_IMAGE_ORIGIN]))
    );
    assert_eq!(
        resource_metadata.pointer("/ui/prefersBorder"),
        Some(&json!(true))
    );

    let resource = tokio::time::timeout(
        PROTOCOL_TIMEOUT,
        client.read_resource(ReadResourceRequestParams::new(APP_RESOURCE_URI)),
    )
    .await
    .context("resources/read timed out")??;
    assert_eq!(resource.contents.len(), 1);
    match &resource.contents[0] {
        ResourceContents::TextResourceContents {
            uri,
            mime_type,
            text,
            meta,
        } => {
            assert_eq!(uri, APP_RESOURCE_URI);
            assert_eq!(mime_type.as_deref(), Some(APP_MIME_TYPE));
            assert!(text.contains("<html"));
            assert!(text.contains(&format!(
                "\"listing_origin\":\"{CONFIGURED_LISTING_ORIGIN}\""
            )));
            assert!(text.contains(&format!("\"image_origin\":\"{CONFIGURED_IMAGE_ORIGIN}\"")));
            assert!(
                !text.contains(&api.uri()),
                "the embedded App must never receive the API base URL"
            );
            assert!(!text.contains("__BUDNA_MCP_PUBLIC_ORIGINS_JSON__"));
            assert!(meta.is_some());
        }
        _ => {
            anyhow::bail!("MCP App resource should be textual HTML")
        }
    }

    let success_cases = [
        (
            "search_listings",
            json!({
                "query": " camera ",
                "category_id": 12,
                "market": "norwegian",
                "min_price": "100.00",
                "max_price": "200",
                "condition": "good",
                "listing_type": "auction",
                "status": "active",
                "ending_soon": true,
                "featured": false,
                "free_shipping": false,
                "sort_by": "price",
                "sort_order": "asc",
                "page": 2,
                "limit": 7,
                "include_facets": true,
                "search_mode": "keyword",
                "location_id": 3,
                "location_region": "Oslo",
                "location_municipality": "Oslo",
                "allow_pickup": true,
                "custom_filters": {"attr_mount": "sony-e"}
            }),
            "/hits/0/id",
            json!(7),
        ),
        ("get_listing", json!({"listing_id": 7}), "/id", json!(7)),
        (
            "get_listing_attributes",
            json!({"listing_id": 7}),
            "/attributes/0/value/type",
            json!("json_display_only"),
        ),
        (
            "get_listing_related",
            json!({"listing_id": 7, "page": 2, "limit": 3}),
            "/listings/0/id",
            json!(8),
        ),
        (
            "get_seller_listings",
            json!({"seller_id": 42, "page": 3, "limit": 4}),
            "/listings/0/seller_id",
            json!(42),
        ),
        (
            "get_categories",
            json!({"page": 2, "limit": 5, "parent_id": 2, "translations": false}),
            "/categories/0/name",
            json!("Cameras"),
        ),
        (
            "get_category_filters",
            json!({"category_id": 12, "translations": false}),
            "/category_id",
            json!(12),
        ),
        (
            "get_filter_options",
            json!({"filter_id": 277, "translations": false}),
            "/options/0/value",
            json!("sony-e"),
        ),
        (
            "get_public_seller_profile",
            json!({"seller_id": 42}),
            "/seller_id",
            json!(42),
        ),
        (
            "get_listing_bid_summary",
            json!({"listing_id": 7}),
            "/bid_count",
            json!(2),
        ),
        (
            "get_public_ratings_summary",
            json!({"listing_id": 7}),
            "/rating_distribution/4",
            json!(7),
        ),
    ];
    let mut successful_outputs = BTreeMap::new();
    for (tool_name, arguments, expected_pointer, expected_value) in success_cases {
        let arguments = arguments
            .as_object()
            .cloned()
            .with_context(|| format!("{tool_name} arguments should be an object"))?;
        let result = tokio::time::timeout(
            PROTOCOL_TIMEOUT,
            client.call_tool(CallToolRequestParams::new(tool_name).with_arguments(arguments)),
        )
        .await
        .with_context(|| format!("{tool_name} tools/call timed out"))??;
        let output_schema = output_schemas
            .get(tool_name)
            .with_context(|| format!("{tool_name} output schema should be retained"))?;
        let output = assert_success_contract(tool_name, &result, output_schema)?;
        assert_eq!(
            output.pointer(expected_pointer),
            Some(&expected_value),
            "{tool_name} should return the expected synthetic fixture"
        );
        successful_outputs.insert(tool_name, output);
    }

    let search = successful_outputs
        .get("search_listings")
        .context("search output should be retained")?;
    assert_eq!(
        search.pointer("/hits/0/listing_url"),
        Some(&json!("https://listings.example.test/l/7"))
    );
    assert_eq!(
        search.pointer("/hits/0/primary_image_url"),
        Some(&json!(
            "https://images.example.test/t/listings/7/thumbs/123e4567-e89b-12d3-a456-426614174000_768x768.webp"
        ))
    );

    let arguments = json!({ "listing_id": 0 })
        .as_object()
        .cloned()
        .context("tool arguments should be an object")?;
    let result = tokio::time::timeout(
        PROTOCOL_TIMEOUT,
        client.call_tool(CallToolRequestParams::new("get_listing").with_arguments(arguments)),
    )
    .await
    .context("tools/call timed out")??;

    assert_tool_error(&result, "get_listing", "INVALID_INPUT", Some(400), false)?;

    let unknown_arguments = json!({"listing_id": 7, "unexpected": true})
        .as_object()
        .cloned()
        .context("unknown-field arguments should be an object")?;
    let unknown_result = tokio::time::timeout(
        PROTOCOL_TIMEOUT,
        client
            .call_tool(CallToolRequestParams::new("get_listing").with_arguments(unknown_arguments)),
    )
    .await
    .context("unknown-field tools/call timed out")??;
    assert_tool_error(
        &unknown_result,
        "get_listing",
        "INVALID_INPUT",
        Some(400),
        false,
    )?;

    let malformed_arguments = json!({ "listing_id": "not-an-integer", "unexpected": true })
        .as_object()
        .cloned()
        .context("malformed tool arguments should be an object")?;
    let malformed_result = tokio::time::timeout(
        PROTOCOL_TIMEOUT,
        client.call_tool(
            CallToolRequestParams::new("get_listing").with_arguments(malformed_arguments),
        ),
    )
    .await
    .context("malformed tools/call timed out")??;
    assert_tool_error(
        &malformed_result,
        "get_listing",
        "INVALID_INPUT",
        Some(400),
        false,
    )?;

    for (listing_id, code, status, retryable) in [
        (400, "INVALID_REQUEST", Some(400), false),
        (404, "LISTING_NOT_FOUND", Some(404), false),
        (429, "BUDNA_API_RETRY_EXHAUSTED", Some(429), true),
        (500, "BUDNA_API_UNAVAILABLE", Some(500), false),
        (600, "BUDNA_API_INVALID_RESPONSE", None, false),
    ] {
        let arguments = json!({"listing_id": listing_id})
            .as_object()
            .cloned()
            .context("error case arguments should be an object")?;
        let result = tokio::time::timeout(
            PROTOCOL_TIMEOUT,
            client.call_tool(CallToolRequestParams::new("get_listing").with_arguments(arguments)),
        )
        .await
        .with_context(|| format!("HTTP {listing_id} error tools/call timed out"))??;
        assert_tool_error(&result, "get_listing", code, status, retryable)?;
    }

    let timeout_arguments = json!({"listing_id": 700})
        .as_object()
        .cloned()
        .context("timeout case arguments should be an object")?;
    let timeout_result = tokio::time::timeout(
        PROTOCOL_TIMEOUT,
        client
            .call_tool(CallToolRequestParams::new("get_listing").with_arguments(timeout_arguments)),
    )
    .await
    .context("timeout error tools/call timed out")??;
    assert_tool_error(
        &timeout_result,
        "get_listing",
        "OPERATION_TIMEOUT",
        Some(504),
        true,
    )?;

    api.verify().await;

    tokio::time::timeout(PROTOCOL_TIMEOUT, client.close())
        .await
        .context("MCP shutdown timed out")??;

    let mut logs = String::new();
    tokio::time::timeout(PROTOCOL_TIMEOUT, stderr.read_to_string(&mut logs))
        .await
        .context("reading child stderr timed out")??;
    assert!(
        !logs.contains("\"jsonrpc\""),
        "protocol messages must not be written to stderr"
    );
    assert!(
        !logs.contains(PRIVATE_SENTINEL),
        "ignored upstream response fields must not be written to trace logs"
    );

    Ok(())
}
