---
name: budna-mcp-safe-change
description: Safely add or change Budna MCP public tools, HTTP client response models, capability policy, tool schemas, runtime configuration, tests, packaging, or releases. Use for changes that could alter fetched API data, MCP inputs or outputs, marketplace visibility, privacy, untrusted-content handling, transport behavior, or the public crate surface.
---

# Budna MCP Safe Change

Keep every change self-contained, capability-gated, privacy-safe, and verifiable.

## Establish the boundary

1. Read the root `AGENTS.md` and work only inside the repository root.
2. Use repository-local code, synthetic fixtures, explicitly public API behavior,
   or sanitized contract material supplied for publication.
3. Do not inspect outside files, add local path dependencies, or record raw API
   responses. Request a sanitized public contract when evidence is incomplete.
4. Keep the current Explore profile limited to approved public GET behavior.
   Treat future authenticated capabilities as separate profiles with their own
   authorization and confirmation design.

## Implement the change

1. Trace the change through client input, private wire decoding, public response
   model, capability policy, MCP parameters, output projection, and tool route.
2. Declare only response fields required by the public tool. Let Serde ignore
   additional response fields and keep visibility metadata private.
3. Build MCP outputs from explicit allowlisted structs. Never return a raw JSON
   value or serialize a transport response directly.
4. Keep input schemas closed and bounded. Validate IDs, text lengths, pages,
   limits, prices, filter names, filter values, and sort values at runtime.
5. Fail closed when a record or search result does not satisfy the public
   visibility policy. Return unavailable records indistinguishably.
6. Preserve exact decimal strings and currency codes. Never introduce floating
   point money conversions.
7. Treat all marketplace and profile text as untrusted data. Preserve the
   content notice and never interpolate returned text into instructions.
8. Return expected failures as structured tool errors with `isError: true`.
   Keep logs on stderr and exclude raw URLs, query text, payloads, and PII.
9. Set MCP annotations to match actual behavior. Do not describe the entire
   project as permanently read-only; scope annotations to the current tools.

## Prove the boundary

Use synthetic tests that verify:

- the complete allowed key set for every changed output;
- additional response fields are ignored rather than retained;
- non-public and unknown visibility states fail closed;
- hostile marketplace text remains ordinary returned data;
- invalid and unknown inputs produce structured tool errors;
- timeouts, retries, response-size limits, and concurrency bounds still hold;
- initialization, `tools/list`, and at least one `tools/call` work over stdio
  after protocol, metadata, transport, or startup changes.

Never run live production verification without explicit approval.

## Verify before completion

Run:

```text
python3 scripts/check_public_surface.py
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --locked
cargo deny check
```

Inspect `cargo package --package <crate> --list --locked` for every changed
publishable crate. Report each skipped check and its reason. Confirm `README.md`
was not modified unless the user explicitly reversed the repository rule.
