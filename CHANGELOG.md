# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- README (French and English) and crate description now present the server as reading and writing Pennylane (invoices, quotes, ledger entries, receipt PDFs), with a dated comparison to Pennylane's own read-only MCP, a non-affiliation notice and a warning that writes are real.

## [0.2.0] - 2026-10-20

### Added

- File uploads: the eight `multipart/form-data` operations (`postFileAttachments`, `postLedgerAttachments`, `createSupplierInvoiceEInvoiceImport`, `createCustomerInvoiceEInvoiceImport`, `createEInvoiceImport`, `postCustomerInvoiceAppendices`, `postQuoteAppendices`, `postCommercialDocumentAppendices`). `file` is the absolute path of a file on the machine running the server; the response carries an `_mcp_pennylane.uploaded_file` audit entry (real path, name, size, type).
- Checks run locally before any byte is sent: absolute path, optional directory confinement, symbolic links resolved before judging, regular file, 100 MB ceiling measured on the open file with a bounded read, file type read from the bytes (PDF, PNG, JPEG, TIFF, BMP, GIF), `filename` of at most 255 characters without control characters.
- `PENNYLANE_UPLOAD_DIRS` / `--upload-dirs`: optional list of directories uploads are confined to (`:`-separated, `;` on Windows, at most 32, resolved at startup). Unset keeps every directory allowed; set it whenever an agent reads untrusted content such as mail.
- `createSupplierInvoiceEInvoiceImport` joins the direct tools: 74 essentials.
- A 403 on a multipart operation lists the scopes it requires; a network failure during an upload says the file may have been received, so the caller checks before sending it again.
- Upload deadline of 60 s plus 1 s per MB instead of a fixed 60 s.
- Bounded uploads: reading the local file times out after 30 s, and at most 4 uploads run at once (400 MB of memory at worst); a fifth waits at most 30 s for a slot, then gets `UPLOADS_BUSY`.

### Fixed

- `postFileAttachments` and `postLedgerAttachments` were listed as tools but could not be used: their schema had no `file` field and the client only sent JSON. The multipart operations are now usable.

### Documentation

- README: file uploads, an upload is not idempotent (reuse the `file_attachment_id`), `invoice_options` of the Factur-X imports is not verified against the real API, and the HTTP transport has no authentication and must not be exposed beyond `127.0.0.1` except behind a reverse proxy that authenticates.

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
