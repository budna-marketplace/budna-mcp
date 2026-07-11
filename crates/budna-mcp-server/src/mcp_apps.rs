use budna_mcp_core::PublicUrlSettings;
use rmcp::{
    ErrorData,
    model::{
        ExtensionCapabilities, ListResourcesResult, Meta, ReadResourceResult, Resource,
        ResourceContents,
    },
};
use serde_json::{Map, Value, json};

pub(crate) const EXTENSION_ID: &str = "io.modelcontextprotocol/ui";
pub(crate) const APP_RESOURCE_URI: &str = "ui://budna/marketplace-explorer-v1.html";
pub(crate) const APP_MIME_TYPE: &str = "text/html;profile=mcp-app";
const APP_RESOURCE_NAME: &str = "budna-marketplace-explorer-v1";
const APP_RESOURCE_TITLE: &str = "Budna Marketplace Explorer";
const APP_RESOURCE_DESCRIPTION: &str =
    "Interactive, read-only cards for public Budna marketplace listings.";
const RUNTIME_CONFIG_MARKER: &str = "__BUDNA_MCP_PUBLIC_ORIGINS_JSON__";

pub(crate) const APP_HTML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/marketplace-explorer-v1.html"
));

pub(crate) fn tool_meta() -> Meta {
    let mut meta = Meta::new();
    meta.0.insert(
        "ui".to_owned(),
        json!({
            "resourceUri": APP_RESOURCE_URI,
            "visibility": ["model", "app"]
        }),
    );
    meta.0.insert(
        "ui/resourceUri".to_owned(),
        Value::String(APP_RESOURCE_URI.to_owned()),
    );
    meta
}

pub(crate) fn extension_capabilities() -> ExtensionCapabilities {
    ExtensionCapabilities::from([(EXTENSION_ID.to_owned(), Map::new())])
}

pub(crate) fn list_resources(public_urls: &PublicUrlSettings) -> ListResourcesResult {
    ListResourcesResult::with_all_items(vec![app_resource(public_urls)])
}

pub(crate) fn read_resource(
    uri: &str,
    public_urls: &PublicUrlSettings,
) -> Result<ReadResourceResult, ErrorData> {
    if uri != APP_RESOURCE_URI {
        return Err(ErrorData::resource_not_found(
            "The requested MCP App resource does not exist",
            None,
        ));
    }

    let contents = ResourceContents::text(render_app_html(public_urls), APP_RESOURCE_URI)
        .with_mime_type(APP_MIME_TYPE)
        .with_meta(resource_meta(public_urls));
    Ok(ReadResourceResult::new(vec![contents]))
}

fn app_resource(public_urls: &PublicUrlSettings) -> Resource {
    let app_html = render_app_html(public_urls);
    Resource::new(APP_RESOURCE_URI, APP_RESOURCE_NAME)
        .with_title(APP_RESOURCE_TITLE)
        .with_description(APP_RESOURCE_DESCRIPTION)
        .with_mime_type(APP_MIME_TYPE)
        .with_size(u64::try_from(app_html.len()).unwrap_or(u64::MAX))
        .with_meta(resource_meta(public_urls))
}

fn resource_meta(public_urls: &PublicUrlSettings) -> Meta {
    let mut meta = Meta::new();
    meta.0.insert(
        "ui".to_owned(),
        json!({
            "prefersBorder": true,
            "csp": {
                "resourceDomains": [public_urls.image_origin()]
            }
        }),
    );
    meta
}

fn render_app_html(public_urls: &PublicUrlSettings) -> String {
    let config = json!({
        "listing_origin": public_urls.listing_origin(),
        "image_origin": public_urls.image_origin(),
    });
    let config = escape_json_for_html_script(config);
    APP_HTML.replace(RUNTIME_CONFIG_MARKER, &config)
}

fn escape_json_for_html_script(value: Value) -> String {
    value
        .to_string()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

#[cfg(test)]
mod tests {
    use budna_mcp_core::{DEFAULT_IMAGE_ORIGIN, PublicUrlSettings};
    use rmcp::model::ResourceContents;
    use serde_json::json;

    use super::*;

    #[test]
    fn tool_metadata_contains_standard_and_compatibility_links() {
        let meta = serde_json::to_value(tool_meta())
            .unwrap_or_else(|error| panic!("tool metadata should serialize: {error}"));

        assert_eq!(
            meta.pointer("/ui/resourceUri"),
            Some(&json!(APP_RESOURCE_URI))
        );
        assert_eq!(
            meta.pointer("/ui/visibility"),
            Some(&json!(["model", "app"]))
        );
        assert_eq!(meta.get("ui/resourceUri"), Some(&json!(APP_RESOURCE_URI)));
        assert_eq!(
            meta.as_object().map(serde_json::Map::len),
            Some(2),
            "tool metadata must not expose undeclared fields"
        );
    }

    #[test]
    fn app_resource_is_single_embedded_html_document_with_strict_csp() {
        assert!(!APP_HTML.trim().is_empty());
        assert!(APP_HTML.len() <= 300 * 1024);
        assert_eq!(APP_HTML.matches(RUNTIME_CONFIG_MARKER).count(), 1);
        let public_urls = PublicUrlSettings::default();
        let rendered_html = render_app_html(&public_urls);
        assert!(!rendered_html.contains(RUNTIME_CONFIG_MARKER));
        assert!(rendered_html.len() <= 300 * 1024);
        let result = list_resources(&public_urls);
        assert_eq!(result.resources.len(), 1);
        let resource = &result.resources[0];
        assert_eq!(resource.uri, APP_RESOURCE_URI);
        assert_eq!(resource.mime_type.as_deref(), Some(APP_MIME_TYPE));
        assert_eq!(
            resource.size,
            Some(u64::try_from(rendered_html.len()).unwrap_or(u64::MAX))
        );

        let metadata = serde_json::to_value(resource.meta.as_ref())
            .unwrap_or_else(|error| panic!("resource metadata should serialize: {error}"));
        assert_eq!(
            metadata.pointer("/ui/csp/resourceDomains"),
            Some(&json!([DEFAULT_IMAGE_ORIGIN]))
        );
        assert_eq!(metadata.pointer("/ui/prefersBorder"), Some(&json!(true)));
        assert!(metadata.pointer("/ui/csp/connectDomains").is_none());
        assert!(metadata.pointer("/ui/csp/frameDomains").is_none());
        assert!(metadata.pointer("/ui/permissions").is_none());

        let read = read_resource(APP_RESOURCE_URI, &public_urls)
            .unwrap_or_else(|error| panic!("known app resource should be readable: {error}"));
        assert_eq!(read.contents.len(), 1);
        match &read.contents[0] {
            ResourceContents::TextResourceContents {
                uri,
                mime_type,
                text,
                meta,
            } => {
                assert_eq!(uri, APP_RESOURCE_URI);
                assert_eq!(mime_type.as_deref(), Some(APP_MIME_TYPE));
                assert_eq!(text, &rendered_html);
                assert!(meta.is_some());
            }
            _ => panic!("MCP App resource should be textual HTML"),
        }
    }

    #[test]
    fn configured_origins_control_csp_and_runtime_ui_config() {
        let public_urls = PublicUrlSettings::new(
            Some("https://listings.example.test".to_owned()),
            Some("https://images.example.test".to_owned()),
        )
        .unwrap_or_else(|error| panic!("configured public URLs should validate: {error}"));

        let resources = list_resources(&public_urls);
        let metadata = serde_json::to_value(resources.resources[0].meta.as_ref())
            .unwrap_or_else(|error| panic!("resource metadata should serialize: {error}"));
        assert_eq!(
            metadata.pointer("/ui/csp/resourceDomains"),
            Some(&json!(["https://images.example.test"]))
        );

        let read = read_resource(APP_RESOURCE_URI, &public_urls)
            .unwrap_or_else(|error| panic!("configured app resource should be readable: {error}"));
        let text = match &read.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => text,
            _ => panic!("MCP App resource should be textual HTML"),
        };
        assert!(text.contains("\"listing_origin\":\"https://listings.example.test\""));
        assert!(text.contains("\"image_origin\":\"https://images.example.test\""));
        assert!(!text.contains(RUNTIME_CONFIG_MARKER));
    }

    #[test]
    fn runtime_config_json_is_safe_inside_an_html_script_element() {
        let escaped = escape_json_for_html_script(json!({
            "value": "</script><script>alert('untrusted')</script>&\u{2028}\u{2029}",
        }));

        assert!(!escaped.contains("</script>"));
        assert!(escaped.contains("\\u003c/script\\u003e"));
        assert!(escaped.contains("\\u0026"));
        assert!(escaped.contains("\\u2028"));
        assert!(escaped.contains("\\u2029"));
    }

    #[test]
    fn unknown_app_resource_uses_resource_not_found_error() {
        let error = match read_resource("ui://budna/unknown.html", &PublicUrlSettings::default()) {
            Ok(_) => panic!("unknown app resource should fail"),
            Err(error) => error,
        };

        assert_eq!(error.code, rmcp::model::ErrorCode::RESOURCE_NOT_FOUND);
        assert_eq!(error.data, None);
    }

    #[test]
    fn server_extension_capability_uses_stable_identifier() {
        let capabilities = extension_capabilities();
        assert_eq!(capabilities.len(), 1);
        assert_eq!(capabilities.get(EXTENSION_ID), Some(&Map::new()));
    }
}
