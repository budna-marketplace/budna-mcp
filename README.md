# Budna MCP

[![build](https://img.shields.io/github/actions/workflow/status/budna-marketplace/budna-mcp/core.yml?branch=main)](https://github.com/budna-marketplace/budna-mcp/actions/workflows/core.yml)
[![crates.io](https://img.shields.io/crates/v/budna-mcp.svg)](https://crates.io/crates/budna-mcp)

Use public Budna marketplace data from any compatible MCP client. Budna MCP is read-only: it does not require an account or API key, and it cannot change marketplace data.

## Quick start

You need [Rust 1.88 or newer](https://rustup.rs/).

Install the latest published release:

```bash
cargo install budna-mcp --locked
```

To install from a local checkout:

```bash
cargo install --path crates/budna-mcp --locked
```

### Add to Codex

On macOS or Linux:

```bash
codex mcp add budna -- "$(command -v budna-mcp)"
codex mcp get budna
```

On Windows PowerShell:

```powershell
$budna = (Get-Command budna-mcp).Source
codex mcp add budna -- $budna
codex mcp get budna
```

Start a new Codex task after adding the server, then ask:

> Use Budna to find public camera listings.

The [Codex MCP documentation](https://learn.chatgpt.com/docs/extend/mcp?surface=cli) covers project-scoped configuration and other client options.

### Add to another stdio MCP client

Configure the absolute path returned by `command -v budna-mcp` (or `(Get-Command budna-mcp).Source` on Windows) with no arguments:

```text
command: /absolute/path/to/budna-mcp
arguments: none
```

For JSON-config clients:

```json
{
  "mcpServers": {
    "budna": {
      "command": "/absolute/path/to/budna-mcp",
      "args": []
    }
  }
}
```

## What you can do

Budna MCP exposes 11 public, read-only tools:

- Discover listings, categories, filters, and filter options.
- Inspect listings, attributes, related listings, seller listings, public seller profiles, bid summaries, and rating summaries.
- In compatible MCP Apps clients, browse interactive cards and explicitly ask the AI to compare two to four selected listings.

It cannot sign in, bid, make offers, buy, message, record views, or mutate marketplace data.

## Use a non-production environment

Configure these values together so API requests, public listing links, and images stay in the same environment:

| Setting | Purpose |
| --- | --- |
| `BUDNA_API_URL` | Budna API base URL, including `/api/v1` if applicable. |
| `BUDNA_PUBLIC_LISTING_ORIGIN` | Origin used for returned listing links. |
| `BUDNA_IMAGE_ORIGIN` | Origin used for returned image URLs and the App CSP. |

Remote API URLs must use HTTPS; HTTP is accepted only for loopback development servers. Listing and image values must be canonical HTTPS origins with no credentials, path, query, fragment, or wildcard host. The binary does not load `.env` files itself; [`.env.example`](.env.example) is reference material.

For example, register a separate Codex server for another environment:

```bash
codex mcp add budna-nonprod -- /absolute/path/to/budna-mcp \
  --api-url https://api.your-environment.example/api/v1 \
  --public-listing-origin https://marketplace.your-environment.example \
  --image-origin https://images.your-environment.example
```

Equivalent environment variables are supported when your client can provide them.

## Interactive cards

The Marketplace Explorer is embedded in the binary; no Node.js runtime is needed to use it. Clients that support the [MCP Apps extension](https://modelcontextprotocol.io/extensions/apps/overview) can render listing cards, details, related/seller listings, and comparison selection. Other clients receive the same text and structured JSON fallback. See the [extension support matrix](https://modelcontextprotocol.io/extensions/client-matrix) for client coverage.

See [the Marketplace Explorer guide](docs/mcp-apps.md) for local HTTP testing, temporary-tunnel safety, privacy and attribution behavior, UI development, and release checks.

The project is licensed under the [MIT License](LICENSE).
