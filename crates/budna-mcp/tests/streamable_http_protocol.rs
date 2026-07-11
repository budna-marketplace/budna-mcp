use std::{collections::BTreeSet, net::Ipv4Addr, process::Stdio, time::Duration};

use anyhow::{Context, Result, bail};
use http::{StatusCode, header};
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ReadResourceRequestParams, ResourceContents},
    transport::StreamableHttpClientTransport,
};
use serde_json::json;
use tokio::io::AsyncReadExt;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

const PROTOCOL_TIMEOUT: Duration = Duration::from_secs(10);
const ALLOWED_ORIGIN: &str = "http://127.0.0.1:8080";
const APP_EXTENSION_ID: &str = "io.modelcontextprotocol/ui";
const APP_RESOURCE_URI: &str = "ui://budna/marketplace-explorer-v1.html";
const INITIALIZE_BODY: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"budna-http-test","version":"1.0"}}}"#;

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

    let port = available_loopback_port().await?;
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_budna-mcp"));
    child
        .arg("--api-url")
        .arg(api.uri())
        .arg("--request-timeout-secs")
        .arg("1")
        .arg("--transport")
        .arg("streamable-http")
        .arg("--http-port")
        .arg(port.to_string())
        .env("BUDNA_MCP_HTTP_ALLOWED_HOSTS", "localhost,127.0.0.1")
        .env(
            "BUDNA_MCP_HTTP_ALLOWED_ORIGINS",
            "http://localhost:8080,http://127.0.0.1:8080",
        )
        .env("RUST_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = child
        .spawn()
        .context("failed to start the packaged budna-mcp HTTP binary")?;
    let mut stderr = child
        .stderr
        .take()
        .context("child stderr should be captured")?;

    wait_for_loopback_server(&mut child, port).await?;
    let endpoint = format!("http://127.0.0.1:{port}/mcp");
    let raw_client = reqwest::Client::new();

    assert_allowed_cors_preflight(&raw_client, &endpoint).await?;
    assert_mcp_response_headers_are_exposed(&raw_client, &endpoint).await?;
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
    assert!(tool_names.contains("get_categories"));
    assert!(tool_names.contains("search_listings"));

    let categories = tokio::time::timeout(
        PROTOCOL_TIMEOUT,
        client.call_tool(CallToolRequestParams::new("get_categories")),
    )
    .await
    .context("Streamable HTTP tools/call timed out")??;
    assert_eq!(categories.is_error, Some(false));
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
    .context("Streamable HTTP App-linked tools/call timed out")??;
    assert_eq!(search.is_error, Some(false));
    assert!(
        search
            .content
            .first()
            .and_then(rmcp::model::ContentBlock::as_text)
            .is_some_and(|content| !content.text.is_empty()),
        "non-App clients need text fallback for App-linked tools"
    );
    assert_eq!(
        search
            .structured_content
            .as_ref()
            .and_then(|value| value.pointer("/hits/0/listing_url")),
        Some(&json!("https://budna.se/l/7"))
    );

    tokio::time::timeout(PROTOCOL_TIMEOUT, client.close())
        .await
        .context("Streamable HTTP MCP shutdown timed out")??;

    shut_down_child(&mut child).await?;

    let mut logs = String::new();
    tokio::time::timeout(PROTOCOL_TIMEOUT, stderr.read_to_string(&mut logs))
        .await
        .context("reading HTTP child stderr timed out")??;
    assert!(
        !logs.contains("\"jsonrpc\""),
        "protocol messages must not be written to stderr"
    );

    Ok(())
}

async fn available_loopback_port() -> Result<u16> {
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .context("failed to reserve a loopback test port")?;
    let port = listener
        .local_addr()
        .context("failed to read the reserved loopback test port")?
        .port();
    drop(listener);
    Ok(port)
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

async fn assert_disallowed_host_is_rejected(
    client: &reqwest::Client,
    endpoint: &str,
) -> Result<()> {
    let response = client
        .post(endpoint)
        .header(header::HOST, "attacker.example")
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
        .header(header::ORIGIN, "https://attacker.example")
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
