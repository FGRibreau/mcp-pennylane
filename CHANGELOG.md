# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-05-08

### Added

- Initial release of `mcp-pennylane` — MCP server for the Pennylane Company API v2.
- ~73 essential tools exposed directly: customers, suppliers, customer/supplier invoices, products, quotes, banking, ledger, exports, file attachments, changelogs, GoCardless and SEPA mandates.
- Two meta-tools for the long tail: `pennylane_search_tools(query)` and `pennylane_execute(tool_name, params)` — together they cover the full 163-operation surface.
- Read-only mode via `PENNYLANE_READONLY=true` env var (or `--readonly` flag) — filters every non-`GET` operation at registration time.
- **Read-only auto-detection**: when `PENNYLANE_READONLY` is unset, the server probes `GET /me` at startup, inspects the token's `scopes` array, and forces read-only mode if every scope ends with `:readonly`. Surfaced in the startup banner as `mode=readonly (auto)` vs `(explicit)` and in the `_mcp_pennylane.readonly_source` field of the augmented `getMe` response.
- Sandbox visual hint via `PENNYLANE_ENV=sandbox` — surfaces in the startup banner, the `getMe` augmentation, and the MCP `serverInfo.instructions`.
- 2026 API breaking changes opt-in via `PENNYLANE_API_2026=true` (sends `X-Use-2026-API-Changes: true` header).
- stdio transport (default) and streamable-http transport (`--transport http`, `/mcp` + `/health`).
- Bearer token auto-redaction in all log levels.
- Rich error mapping (code, suggestion, truncated upstream body) for 4xx/5xx Pennylane responses.
- Single retry on 429 with `retry-after` header support, capped at 60 s.
- OpenAPI spec vendored at `openapi/accounting.json`; `cargo run -p refresh-openapi` to refresh; weekly GitHub Action opens a PR if upstream drifts.
- Release pipeline gated behind a `release` GitHub Actions environment with a tag-only deployment policy (`v*`) — privileged secrets cannot leak to feature branches.
