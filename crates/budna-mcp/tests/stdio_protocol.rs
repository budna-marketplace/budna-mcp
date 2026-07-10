use std::{process::Stdio, time::Duration};

use anyhow::{Context, Result};
use rmcp::{
    ServiceExt,
    model::CallToolRequestParams,
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use serde_json::json;
use tokio::io::AsyncReadExt;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

const PROTOCOL_TIMEOUT: Duration = Duration::from_secs(10);

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

    let command = tokio::process::Command::new(env!("CARGO_BIN_EXE_budna-mcp")).configure(|cmd| {
        cmd.arg("--api-url")
            .arg(api.uri())
            .arg("--request-timeout-secs")
            .arg("1")
            .env("RUST_LOG", "warn");
    });
    let (transport, stderr) = TokioChildProcess::builder(command)
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to start the packaged budna-mcp binary")?;
    let mut stderr = stderr.context("child stderr should be captured")?;

    let mut client = tokio::time::timeout(PROTOCOL_TIMEOUT, ().serve(transport))
        .await
        .context("MCP initialization timed out")??;

    let server_info = client
        .peer_info()
        .context("MCP initialization should return server information")?;
    assert_eq!(server_info.server_info.name, "budna-mcp");
    assert!(server_info.capabilities.tools.is_some());
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
    }

    let categories = tokio::time::timeout(
        PROTOCOL_TIMEOUT,
        client.call_tool(CallToolRequestParams::new("get_categories")),
    )
    .await
    .context("successful tools/call timed out")??;
    assert_eq!(categories.is_error, Some(false));
    assert_eq!(
        categories
            .structured_content
            .as_ref()
            .and_then(|value| value.pointer("/categories/0/name")),
        Some(&json!("Cameras"))
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
