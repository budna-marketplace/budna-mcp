use std::{
    collections::{BTreeMap, BTreeSet},
    net::{Ipv4Addr, SocketAddr},
    process::Stdio,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use http::{StatusCode, header};
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ReadResourceRequestParams, ResourceContents},
    transport::StreamableHttpClientTransport,
};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

const PROTOCOL_TIMEOUT: Duration = Duration::from_secs(10);
const ALLOWED_ORIGIN: &str = "http://127.0.0.1:8080";
const EXACT_CONFIGURED_ORIGIN: &str = "https://app.example.test";
const DIFFERENT_PORT_ORIGIN: &str = "https://app.example.test:4444";
const DISALLOWED_ORIGIN: &str = "https://attacker.example/?origin_secret_sentinel";
const DISALLOWED_HOST: &str = "host-secret-sentinel.invalid";
const APP_EXTENSION_ID: &str = "io.modelcontextprotocol/ui";
const APP_RESOURCE_URI: &str = "ui://budna/marketplace-explorer-v1.html";
const PRIVATE_SENTINEL: &str = "private-http-contract-sentinel";
const INITIALIZE_BODY: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"budna-http-test","version":"1.0"}}}"#;

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
    assert!(!structured.to_string().contains(PRIVATE_SENTINEL));
    Ok(structured.clone())
}

#[tokio::test]
async fn packaged_binary_serves_stateful_loopback_streamable_http_securely() -> Result<()> {
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
                    "translations": {"name": {"en": "Cameras", "sv": "Kameror", "no": "Kameraer"}},
                    "server_only_marker": PRIVATE_SENTINEL
                }],
                "pagination": {"page": 1, "limit": 100, "total": 1, "total_pages": 1},
                "server_only_marker": PRIVATE_SENTINEL
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
                    "has_bids": true,
                    "server_only_marker": PRIVATE_SENTINEL
                }],
                "total": 1,
                "page": 1,
                "per_page": 10,
                "total_pages": 1,
                "search_time_ms": 3,
                "facets": null,
                "server_only_marker": PRIVATE_SENTINEL
            }
        })))
        .mount(&api)
        .await;

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_budna-mcp"));
    child
        .arg("--api-url")
        .arg(api.uri())
        .arg("--request-timeout-secs")
        .arg("1")
        .arg("--transport")
        .arg("streamable-http")
        .arg("--http-port")
        .arg("0")
        .env("BUDNA_MCP_HTTP_ALLOWED_HOSTS", "localhost,127.0.0.1")
        .env(
            "BUDNA_MCP_HTTP_ALLOWED_ORIGINS",
            "http://localhost:8080,http://127.0.0.1:8080,https://app.example.test",
        )
        .env("BUDNA_MCP_LOG", "trace")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = child
        .spawn()
        .context("failed to start the packaged budna-mcp HTTP binary")?;
    let stderr = child
        .stderr
        .take()
        .context("child stderr should be captured")?;
    let mut stderr = BufReader::new(stderr);
    let mut logs = String::new();
    let address = read_http_listen_address(&mut stderr, &mut logs).await?;
    let port = address.port();

    wait_for_loopback_server(&mut child, port).await?;
    let endpoint = format!("http://127.0.0.1:{port}/mcp");
    let raw_client = reqwest::Client::new();

    assert_allowed_cors_preflight(&raw_client, &endpoint).await?;
    assert_mcp_response_headers_are_exposed(&raw_client, &endpoint).await?;
    assert_exact_configured_origin_is_accepted(&raw_client, &endpoint).await?;
    assert_same_host_different_port_origin_is_rejected(&raw_client, &endpoint).await?;
    assert_disallowed_host_is_rejected(&raw_client, &endpoint).await?;
    assert_disallowed_origin_is_rejected(&raw_client, &endpoint).await?;
    assert_oversized_body_is_rejected(&raw_client, &endpoint).await?;

    let transport = StreamableHttpClientTransport::from_uri(endpoint.clone());
    let mut client = tokio::time::timeout(PROTOCOL_TIMEOUT, ().serve(transport))
        .await
        .context("Streamable HTTP MCP initialization timed out")??;

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
            .and_then(|extensions| extensions.get(APP_EXTENSION_ID))
            .is_some_and(serde_json::Map::is_empty)
    );

    let resources = tokio::time::timeout(PROTOCOL_TIMEOUT, client.list_all_resources())
        .await
        .context("Streamable HTTP resources/list timed out")??;
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].uri, APP_RESOURCE_URI);

    let app_resource = tokio::time::timeout(
        PROTOCOL_TIMEOUT,
        client.read_resource(ReadResourceRequestParams::new(APP_RESOURCE_URI)),
    )
    .await
    .context("Streamable HTTP resources/read timed out")??;
    assert_eq!(app_resource.contents.len(), 1);
    match &app_resource.contents[0] {
        ResourceContents::TextResourceContents {
            uri,
            mime_type,
            text,
            ..
        } => {
            assert_eq!(uri, APP_RESOURCE_URI);
            assert_eq!(mime_type.as_deref(), Some("text/html;profile=mcp-app"));
            assert!(text.contains("<!"), "MCP App resource should contain HTML");
        }
        _ => bail!("MCP App resource should be a textual document"),
    }

    let tools = tokio::time::timeout(PROTOCOL_TIMEOUT, client.list_all_tools())
        .await
        .context("Streamable HTTP tools/list timed out")??;
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
    let mut output_schemas = BTreeMap::new();
    for tool in &tools {
        assert_eq!(
            tool.input_schema.get("type"),
            Some(&json!("object")),
            "{} input schema should describe an object",
            tool.name
        );
        assert_eq!(
            tool.input_schema.get("additionalProperties"),
            Some(&json!(false)),
            "{} input schema should reject unknown fields",
            tool.name
        );
        let output_schema = tool
            .output_schema
            .as_ref()
            .with_context(|| format!("{} should advertise an output schema", tool.name))?;
        let output_schema = serde_json::Value::Object((**output_schema).clone());
        assert_eq!(output_schema.get("type"), Some(&json!("object")));
        assert!(
            output_schema
                .get("properties")
                .is_some_and(serde_json::Value::is_object)
        );
        output_schemas.insert(tool.name.to_string(), output_schema);

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
    .context("Streamable HTTP tools/call timed out")??;
    let categories = assert_success_contract(
        "get_categories",
        &categories,
        output_schemas
            .get("get_categories")
            .context("category output schema should be retained")?,
    )?;
    assert_eq!(
        categories.pointer("/categories/0/name"),
        Some(&json!("Cameras"))
    );

    let search = tokio::time::timeout(
        PROTOCOL_TIMEOUT,
        client.call_tool(
            CallToolRequestParams::new("search_listings").with_arguments(serde_json::Map::new()),
        ),
    )
    .await
    .context("Streamable HTTP App-linked tools/call timed out")??;
    let search = assert_success_contract(
        "search_listings",
        &search,
        output_schemas
            .get("search_listings")
            .context("search output schema should be retained")?,
    )?;
    assert_eq!(
        search.pointer("/hits/0/listing_url"),
        Some(&json!("https://budna.se/l/7"))
    );

    tokio::time::timeout(PROTOCOL_TIMEOUT, client.close())
        .await
        .context("Streamable HTTP MCP shutdown timed out")??;

    shut_down_child(&mut child).await?;

    tokio::time::timeout(PROTOCOL_TIMEOUT, stderr.read_to_string(&mut logs))
        .await
        .context("reading HTTP child stderr timed out")??;
    assert!(
        !logs.contains("\"jsonrpc\""),
        "protocol messages must not be written to stderr"
    );
    assert!(
        !logs.contains("origin_secret_sentinel") && !logs.contains("host-secret-sentinel"),
        "rejected request headers must not be written to stderr"
    );
    assert!(
        !logs.contains(PRIVATE_SENTINEL),
        "ignored upstream response fields must not be written to stderr"
    );

    Ok(())
}

async fn read_http_listen_address(
    stderr: &mut BufReader<tokio::process::ChildStderr>,
    logs: &mut String,
) -> Result<SocketAddr> {
    tokio::time::timeout(PROTOCOL_TIMEOUT, async {
        loop {
            let mut line = String::new();
            let bytes_read = stderr
                .read_line(&mut line)
                .await
                .context("failed to read HTTP server startup log")?;
            if bytes_read == 0 {
                bail!("HTTP server exited before reporting its loopback address");
            }

            logs.push_str(&line);
            if let Some(address) = line
                .split_whitespace()
                .find_map(|field| field.strip_prefix("bind_address="))
            {
                return address
                    .trim_matches('"')
                    .parse()
                    .context("HTTP server reported an invalid loopback address");
            }
        }
    })
    .await
    .context("HTTP server did not report its loopback address in time")?
}

async fn wait_for_loopback_server(child: &mut tokio::process::Child, port: u16) -> Result<()> {
    for _ in 0..100 {
        if let Some(status) = child
            .try_wait()
            .context("failed to inspect HTTP server process")?
        {
            bail!("HTTP server exited before accepting connections: {status}");
        }

        if tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .is_ok()
        {
            return Ok(());
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    bail!("HTTP server did not accept a loopback connection in time")
}

async fn assert_allowed_cors_preflight(client: &reqwest::Client, endpoint: &str) -> Result<()> {
    let response = client
        .request(reqwest::Method::OPTIONS, endpoint)
        .header(header::ORIGIN, ALLOWED_ORIGIN)
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
        .header(
            header::ACCESS_CONTROL_REQUEST_HEADERS,
            "content-type,mcp-session-id,mcp-protocol-version,last-event-id",
        )
        .send()
        .await
        .context("failed to send allowed CORS preflight")?;

    assert!(response.status().is_success());
    assert_eq!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok()),
        Some(ALLOWED_ORIGIN)
    );

    let allowed_headers = response
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
        .and_then(|value| value.to_str().ok())
        .context("CORS preflight should return allowed headers")?
        .to_ascii_lowercase();
    for required in [
        "content-type",
        "mcp-session-id",
        "mcp-protocol-version",
        "last-event-id",
    ] {
        assert!(
            allowed_headers.contains(required),
            "CORS response should allow {required}: {allowed_headers}"
        );
    }

    Ok(())
}

async fn assert_mcp_response_headers_are_exposed(
    client: &reqwest::Client,
    endpoint: &str,
) -> Result<()> {
    let response = client
        .get(endpoint)
        .header(header::ORIGIN, ALLOWED_ORIGIN)
        .header(header::ACCEPT, "text/event-stream")
        .send()
        .await
        .context("failed to send regular CORS request")?;

    let exposed_headers = response
        .headers()
        .get(header::ACCESS_CONTROL_EXPOSE_HEADERS)
        .and_then(|value| value.to_str().ok())
        .context("CORS response should expose MCP response headers")?
        .to_ascii_lowercase();
    for required in ["mcp-session-id", "mcp-protocol-version"] {
        assert!(
            exposed_headers.contains(required),
            "CORS response should expose {required}: {exposed_headers}"
        );
    }

    Ok(())
}

async fn assert_exact_configured_origin_is_accepted(
    client: &reqwest::Client,
    endpoint: &str,
) -> Result<()> {
    let response = client
        .post(endpoint)
        .header(header::ORIGIN, EXACT_CONFIGURED_ORIGIN)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream")
        .body(INITIALIZE_BODY)
        .send()
        .await
        .context("failed to send request with an exact configured Origin")?;

    assert!(response.status().is_success());
    assert_eq!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok()),
        Some(EXACT_CONFIGURED_ORIGIN)
    );
    Ok(())
}

async fn assert_same_host_different_port_origin_is_rejected(
    client: &reqwest::Client,
    endpoint: &str,
) -> Result<()> {
    let preflight = client
        .request(reqwest::Method::OPTIONS, endpoint)
        .header(header::ORIGIN, DIFFERENT_PORT_ORIGIN)
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
        .send()
        .await
        .context("failed to send different-port CORS preflight")?;
    assert_eq!(preflight.status(), StatusCode::FORBIDDEN);

    let response = client
        .post(endpoint)
        .header(header::ORIGIN, DIFFERENT_PORT_ORIGIN)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream")
        .body(INITIALIZE_BODY)
        .send()
        .await
        .context("failed to send different-port Origin request")?;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(response.headers().get("mcp-session-id").is_none());
    Ok(())
}

async fn assert_disallowed_host_is_rejected(
    client: &reqwest::Client,
    endpoint: &str,
) -> Result<()> {
    let response = client
        .post(endpoint)
        .header(header::HOST, DISALLOWED_HOST)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream")
        .body(INITIALIZE_BODY)
        .send()
        .await
        .context("failed to send request with a disallowed Host")?;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    Ok(())
}

async fn assert_disallowed_origin_is_rejected(
    client: &reqwest::Client,
    endpoint: &str,
) -> Result<()> {
    let response = client
        .post(endpoint)
        .header(header::ORIGIN, DISALLOWED_ORIGIN)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream")
        .body(INITIALIZE_BODY)
        .send()
        .await
        .context("failed to send request with a disallowed Origin")?;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    Ok(())
}

async fn assert_oversized_body_is_rejected(client: &reqwest::Client, endpoint: &str) -> Result<()> {
    let response = client
        .post(endpoint)
        .header(header::ORIGIN, ALLOWED_ORIGIN)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream")
        .body(vec![b' '; 64 * 1024 + 1])
        .send()
        .await
        .context("failed to send oversized HTTP request")?;

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    Ok(())
}

#[cfg(unix)]
async fn shut_down_child(child: &mut tokio::process::Child) -> Result<()> {
    let process_id = child
        .id()
        .context("HTTP server process ID is unavailable")?;
    let status = tokio::process::Command::new("kill")
        .arg("-INT")
        .arg(process_id.to_string())
        .status()
        .await
        .context("failed to send interrupt to HTTP server")?;
    if !status.success() {
        bail!("interrupt command failed with status {status}");
    }

    let status = tokio::time::timeout(PROTOCOL_TIMEOUT, child.wait())
        .await
        .context("HTTP server did not shut down after interrupt")??;
    if !status.success() {
        bail!("HTTP server exited unsuccessfully: {status}");
    }
    Ok(())
}

#[cfg(not(unix))]
async fn shut_down_child(child: &mut tokio::process::Child) -> Result<()> {
    child
        .start_kill()
        .context("failed to terminate HTTP test server")?;
    tokio::time::timeout(PROTOCOL_TIMEOUT, child.wait())
        .await
        .context("HTTP test server did not terminate")??;
    Ok(())
}
