# Budna Marketplace Explorer

Budna MCP includes an interactive marketplace explorer built with the standard
[MCP Apps extension](https://modelcontextprotocol.io/extensions/apps/overview).
Compatible hosts can render listing results as cards inside the conversation.
Other MCP clients continue to receive the same structured JSON and text
fallback, so the UI is always progressive enhancement.

## User experience

Ask an MCP client to find public Budna listings. In a host with MCP Apps
support, search results, related listings, seller listings, and individual
listing details use the Marketplace Explorer view.

The view can:

- show listing images, exact prices, condition, shipping, bids, and end times;
- open a result's public details without leaving the conversation first;
- refresh a listing and browse related or same-seller listings;
- load more results up to a bounded 50-listing view; and
- send two to four explicitly selected listings back to the AI for comparison.

The explorer cannot sign in, bid, make an offer, buy, message, or change any
Budna resource.

## Compatibility

MCP Apps is optional and host support varies. Consult the official
[extension support matrix](https://modelcontextprotocol.io/extensions/client-matrix)
for current client coverage.

The server always returns meaningful text plus schema-conforming
`structuredContent`. Clients that do not negotiate the UI extension therefore
retain the normal Budna MCP experience.

## Local stdio use

The default transport remains `stdio`, and no Node.js runtime is required by
people installing the published crate:

```bash
cargo install budna-mcp --locked
budna-mcp
```

Configure the installed executable as a local MCP server using the instructions
in the project README. A local host that supports MCP Apps can fetch the
embedded `ui://budna/marketplace-explorer-v1.html` resource directly from this
connection.

## Environment endpoints

Keep these values aligned for the environment being explored:

```text
BUDNA_API_URL=https://api.your-environment.example/api/v1
BUDNA_PUBLIC_LISTING_ORIGIN=https://marketplace.your-environment.example
BUDNA_IMAGE_ORIGIN=https://images.your-environment.example
```

`BUDNA_API_URL` is used only by the MCP server for public API requests.
`BUDNA_PUBLIC_LISTING_ORIGIN` is used for public listing URLs, and
`BUDNA_IMAGE_ORIGIN` is used for derived image URLs and the App's image-only
CSP. The embedded App never receives the API origin and makes no direct API
request.

The public listing and image settings must be canonical HTTPS origins: no
credentials, path, query, fragment, or wildcard host. The same settings are
available as `--api-url`, `--public-listing-origin`, and `--image-origin`.

## Local Streamable HTTP use

The optional HTTP mode exists for local development and host testing:

```bash
budna-mcp --transport streamable-http --http-port 3001
```

It listens only on `127.0.0.1` and exposes the MCP endpoint at:

```text
http://127.0.0.1:3001/mcp
```

The default Host allowlist contains `localhost` and `127.0.0.1`. The default
browser Origin allowlist contains the standard local addresses used by the
official MCP Apps basic host on port 8080. Additional values must be explicit:

```bash
budna-mcp \
  --transport streamable-http \
  --http-allowed-host connector.example.com \
  --http-allowed-origin https://host.example.com
```

The corresponding environment variables are:

```text
BUDNA_MCP_HTTP_PORT
BUDNA_MCP_HTTP_ALLOWED_HOSTS
BUDNA_MCP_HTTP_ALLOWED_ORIGINS
```

Host and Origin lists are comma-delimited. Wildcard origins and public bind
addresses are not supported.

## Test with the official basic host

Follow the official
[MCP Apps build guide](https://modelcontextprotocol.io/extensions/apps/build)
to install the `modelcontextprotocol/ext-apps` basic host, then start it with
the Budna endpoint:

```bash
SERVERS='["http://127.0.0.1:3001/mcp"]' npm start
```

The basic host normally runs at `http://localhost:8080`. Search for listings,
call one of the UI-enabled tools, and verify the card, detail, navigation, and
comparison experiences in light and dark themes.

## Temporary tunnels

An HTTPS tunnel may be useful when a remote development host cannot reach the
loopback server. This is a temporary testing workflow, not a production
deployment.

Before starting Budna MCP, add the tunnel's exact hostname or authority with
`--http-allowed-host` and add the remote browser host's exact origin with
`--http-allowed-origin`. Keep the listener on `127.0.0.1`, use a short-lived
tunnel, and stop both processes after testing. The local HTTP profile is
unauthenticated because it exposes only the existing public Explore tools; it
is not hardened or operated as an internet-facing service.

Production hosting requires a separate design for deployment, authorization,
abuse prevention, monitoring, and operational ownership.

## Privacy and attribution

The embedded view performs no direct API requests and uses no cookies, local
storage, camera, microphone, geolocation, clipboard permission, or hidden
telemetry. Images are the only remotely loaded resources and are restricted by
the declared MCP App content security policy to the configured
`BUDNA_IMAGE_ORIGIN` (which defaults to `https://images.budna.se`). Image
requests use `no-referrer`.

When a user explicitly opens a listing, the view asks the host to open the
clean public listing URL with these fixed parameters:

```text
utm_source=budna_mcp
utm_medium=ai_assistant
utm_campaign=interactive_cards
```

No user, client, conversation, or MCP session identifier is included.

## Develop the UI

UI source lives in `ui/marketplace-explorer`. Node.js 22.13 or newer is
required for UI development only.

```bash
npm --prefix ui/marketplace-explorer ci
npm --prefix ui/marketplace-explorer run check
npm --prefix ui/marketplace-explorer run build
```

The Vite build produces one self-contained HTML document embedded by the Rust
server. The generated file is committed so crates.io consumers do not need
Node.js. CI rebuilds it and rejects drift, source maps, oversized assets, and
missing third-party notices.

Marketplace text is untrusted. Rendering code must continue to use safe DOM
text APIs, validate derived URLs, and avoid interpreting titles, descriptions,
tags, seller names, or location labels as HTML, Markdown, or instructions.
