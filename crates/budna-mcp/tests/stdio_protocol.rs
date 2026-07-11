use std::{process::Stdio, time::Duration};

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
    matchers::{method, path},
};

const PROTOCOL_TIMEOUT: Duration = Duration::from_secs(10);
const APP_EXTENSION_ID: &str = "io.modelcontextprotocol/ui";
const APP_RESOURCE_URI: &str = "ui://budna/marketplace-explorer-v1.html";
const APP_MIME_TYPE: &str = "text/html;profile=mcp-app";
const CONFIGURED_IMAGE_ORIGIN: &str = "https://images.example.test";
const CONFIGURED_LISTING_ORIGIN: &str = "https://listings.example.test";

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
    serde_json::from_str(&content.text).context("tool error content should be JSON")
}

#[tokio::test]
async fn packaged_binary_serves_the_public_explore_profile_over_stdio() -> Result<()> {
    let api = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/categories"))
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
        .mount(&api)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/search/listings"))
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
                    "has_bids": true
                }],
                "total": 1,
                "page": 1,
                "per_page": 10,
                "total_pages": 1,
                "search_time_ms": 3,
                "facets": null
            }
        })))
        .mount(&api)
        .await;

    let command = tokio::process::Command::new(env!("CARGO_BIN_EXE_budna-mcp")).configure(|cmd| {
        cmd.arg("--api-url")
            .arg(api.uri())
            .arg("--request-timeout-secs")
            .arg("1")
            .env("BUDNA_PUBLIC_LISTING_ORIGIN", CONFIGURED_LISTING_ORIGIN)
            .env("BUDNA_IMAGE_ORIGIN", CONFIGURED_IMAGE_ORIGIN)
            .env("RUST_LOG", "warn");
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
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        tool_names,
        std::collections::BTreeSet::from([
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

    let app_tool_names = std::collections::BTreeSet::from([
        "get_listing",
        "get_listing_related",
        "get_seller_listings",
        "search_listings",
    ]);
    for tool in tools {
        assert!(
            tool.output_schema.is_some(),
            "{} should advertise an output schema",
            tool.name
        );
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

    let categories = tokio::time::timeout(
        PROTOCOL_TIMEOUT,
        client.call_tool(CallToolRequestParams::new("get_categories")),
    )
    .await
    .context("successful tools/call timed out")??;
    assert_eq!(categories.is_error, Some(false));
    assert!(
        categories
            .content
            .first()
            .and_then(rmcp::model::ContentBlock::as_text)
            .is_some_and(|content| !content.text.is_empty()),
        "non-App clients need a meaningful text fallback"
    );
    assert_eq!(
        categories
            .structured_content
            .as_ref()
            .and_then(|value| value.pointer("/categories/0/name")),
        Some(&json!("Cameras"))
    );

    let search = tokio::time::timeout(
        PROTOCOL_TIMEOUT,
        client.call_tool(
            CallToolRequestParams::new("search_listings").with_arguments(serde_json::Map::new()),
        ),
    )
    .await
    .context("App-linked fallback tools/call timed out")??;
    assert_eq!(search.is_error, Some(false));
    assert!(
        search
            .content
            .first()
            .and_then(rmcp::model::ContentBlock::as_text)
            .is_some_and(|content| !content.text.is_empty()),
        "App-linked tools must retain a meaningful text fallback"
    );
    assert_eq!(
        search
            .structured_content
            .as_ref()
            .and_then(|value| value.pointer("/hits/0/listing_url")),
        Some(&json!("https://listings.example.test/l/7"))
    );
    assert_eq!(
        search
            .structured_content
            .as_ref()
            .and_then(|value| value.pointer("/hits/0/primary_image_url")),
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

    assert_eq!(result.is_error, Some(true));
    assert!(result.structured_content.is_none());
    assert_eq!(
        error_payload(&result)?.pointer("/error/code"),
        Some(&json!("INVALID_INPUT"))
    );

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
    assert_eq!(malformed_result.is_error, Some(true));
    assert_eq!(
        error_payload(&malformed_result)?.pointer("/error/code"),
        Some(&json!("INVALID_INPUT"))
    );

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

    Ok(())
}
