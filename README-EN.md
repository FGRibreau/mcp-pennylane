<p align="right"><a href="README.md">🇫🇷 Français</a> · <b>🇬🇧 English</b></p>

<div align="center">

# mcp-pennylane

**Drive your Pennylane accounting from Claude — invoices, ledger, banking, FEC exports**

<br/>

<img src="assets/banner.svg" alt="Claude → mcp-pennylane → Pennylane Company API v2" width="700"/>

<br/>
<br/>

[![crates.io](https://img.shields.io/crates/v/mcp-pennylane.svg)](https://crates.io/crates/mcp-pennylane)
[![downloads](https://img.shields.io/crates/d/mcp-pennylane.svg)](https://crates.io/crates/mcp-pennylane)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![CI](https://github.com/FGRibreau/mcp-pennylane/actions/workflows/ci.yml/badge.svg)](https://github.com/FGRibreau/mcp-pennylane/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://rust-lang.org)
[![MCP](https://img.shields.io/badge/MCP-2024--11--05-6366f1.svg)](https://modelcontextprotocol.io)

</div>

---

## Sponsors

<table>
  <tr>
    <td align="center" width="175">
      <a href="https://france-nuage.fr/?mtm_source=github&mtm_medium=sponsor&mtm_campaign=france-nuage&mtm_content=mcp-pennylane">
        <img src="assets/sponsors/france-nuage.svg" height="60" alt="France-Nuage"/><br/>
        <b>France-Nuage</b>
      </a><br/>
      <sub>Sovereign French cloud to host your accounting exports and FEC archives.</sub>
    </td>
    <td align="center" width="175">
      <a href="https://hook0.com/?mtm_source=github&mtm_medium=sponsor&mtm_campaign=hook0&mtm_content=mcp-pennylane">
        <img src="assets/sponsors/hook0.png" height="60" alt="Hook0"/><br/>
        <b>Hook0</b>
      </a><br/>
      <sub>Forward Pennylane webhooks as signed events to your back-office stack.</sub>
    </td>
    <td align="center" width="175">
      <a href="https://getnatalia.com/?mtm_source=github&mtm_medium=sponsor&mtm_campaign=natalia&mtm_content=mcp-pennylane">
        <img src="assets/sponsors/natalia.svg" height="60" alt="Natalia"/><br/>
        <b>Natalia</b>
      </a><br/>
      <sub>AI voice agent that fields supplier and customer calls about invoices.</sub>
    </td>
    <td align="center" width="175">
      <a href="https://www.netir.fr/?mtm_source=github&mtm_medium=sponsor&mtm_campaign=netir&mtm_content=mcp-pennylane">
        <img src="assets/sponsors/netir.svg" height="60" alt="Netir"/><br/>
        <b>Netir</b>
      </a><br/>
      <sub>Hire vetted French freelance accountants and finance ops engineers.</sub>
    </td>
  </tr>
  <tr>
    <td align="center" width="233">
      <a href="https://nobullshitconseil.com/?mtm_source=github&mtm_medium=sponsor&mtm_campaign=nbc&mtm_content=mcp-pennylane">
        <img src="assets/sponsors/nobullshitconseil.svg" height="60" alt="NoBullshitConseil"/><br/>
        <b>NoBullshitConseil</b>
      </a><br/>
      <sub>Finance ops and ERP advisory without the bullshit. Pennylane integrations.</sub>
    </td>
    <td align="center" width="233">
      <a href="https://qualneo.fr/?mtm_source=github&mtm_medium=sponsor&mtm_campaign=qualneo&mtm_content=mcp-pennylane">
        <img src="assets/sponsors/qualneo.svg" height="60" alt="Qualneo"/><br/>
        <b>Qualneo</b>
      </a><br/>
      <sub>Qualiopi LMS for French trainers, with Pennylane-ready billing exports.</sub>
    </td>
    <td align="center" width="233">
      <a href="https://www.recapro.ai/?mtm_source=github&mtm_medium=sponsor&mtm_campaign=recapro&mtm_content=mcp-pennylane">
        <img src="assets/sponsors/recapro.png" height="60" alt="Recapro"/><br/>
        <b>Recapro</b>
      </a><br/>
      <sub>Private AI to transcribe accountant meetings and draft client memos on-prem.</sub>
    </td>
  </tr>
</table>

> **Interested in sponsoring?** [Get in touch](mailto:rust@fgribreau.com)

---

## What is this?

`mcp-pennylane` connects Claude (or any MCP host) to the [Pennylane Company API v2](https://pennylane.readme.io). About 73 hand-curated essentials are listed directly to the host, and two meta-tools (`pennylane_search_tools`, `pennylane_execute`) cover the long tail — so the full 163-op surface stays usable without flooding the host's tool budget.

Drive your accounting from a chat: list and create customer invoices, reconcile bank transactions, query the ledger, generate FEC exports for your accountant, manage GoCardless and SEPA mandates, all from natural language.

## Features

- ✨ **All 163 Pennylane ops** — 73 essentials direct + `pennylane_search_tools` + `pennylane_execute` for the long tail
- 🔒 **Read-only auto-detect** — probes `GET /me` at startup, forces read-only when every token scope ends with `:readonly`
- ⚡ **Two transports** — stdio for Claude Desktop, streamable HTTP for remote (`/mcp` + `/health`)
- ⚙️ **OpenAPI-driven codegen** — `build.rs` parses the vendored spec, fail-fast on whitelist drift
- 🤖 **Weekly auto-PR** — GitHub Action diffs upstream every Monday and opens a PR if the spec drifted
- 🛡️ **Token redaction** — `Bearer abcd***wxyz` on every log path, accounting bodies never logged at INFO
- 💡 **Rich error mapping** — `UNAUTHORIZED`, `VALIDATION_FAILED`, `RATE_LIMITED`, … with truncated upstream body and actionable hints
- 📦 **Single static binary** — `cargo install`, GitHub Releases (linux/macOS/windows × x86_64/aarch64), Homebrew tap

## Quick Start

```bash
# 1. Install
cargo install mcp-pennylane

# 2. Generate a Pennylane API token in Settings → Connectivity → Developers
#    Recommended scope: "Read only — retrieve data"
export PENNYLANE_API_KEY="your-pennylane-token"

# 3. Wire it into Claude Desktop, then restart Claude
cat <<'EOF' >> ~/Library/Application\ Support/Claude/claude_desktop_config.json
{
  "mcpServers": {
    "pennylane": {
      "command": "/usr/local/bin/mcp-pennylane",
      "env": { "PENNYLANE_API_KEY": "your-pennylane-token" }
    }
  }
}
EOF
```

That's it — Claude now sees `getMe`, `getCustomers`, `pennylane_search_tools`, and 70+ other tools.

> 💡 The server probes `GET /me` at startup. If your token is read-only, it auto-enables read-only mode (banner reads `mode=readonly (auto)`). Set `PENNYLANE_READONLY=true` to skip the probe and force read-only deterministically (recommended for CI).

## Configuration

### Environment variables

| Variable | Default | Purpose |
|---|---|---|
| `PENNYLANE_API_KEY` | *required* | Bearer token (Settings → Connectivity → Developers) |
| `PENNYLANE_BASE_URL` | `https://app.pennylane.com` | Override for proxy or hypothetical regional URL |
| `PENNYLANE_READONLY` | *(auto-detected)* | `true` / `false` to skip the `/me` scope probe |
| `PENNYLANE_ENV` | `production` | `production` / `sandbox` — visual cue in banner and `getMe` |
| `PENNYLANE_API_2026` | `false` | Send `X-Use-2026-API-Changes: true` (preview phase) |
| `MCP_PENNYLANE_TRANSPORT` | `stdio` | `stdio` or `http` |
| `RUST_LOG` | `info` | Standard `tracing-subscriber` env filter |

### CLI flags

Every env var has a matching `--flag` (e.g. `--token`, `--base-url`, `--readonly`, `--env`, `--api-2026`, `--transport`, `--host`, `--port`, `--log-level`). Run `mcp-pennylane --help` for the full list.

## Install

| Channel | Command |
|---|---|
| crates.io | `cargo install mcp-pennylane` |
| Homebrew | `brew install fgribreau/tap/mcp-pennylane` |
| GitHub Releases | [Download tarball](https://github.com/FGRibreau/mcp-pennylane/releases/latest) (linux/macOS/windows × x86_64/aarch64) |

## Usage

Three end-to-end workflows you can drive from Claude:

**1. List recent unpaid invoices.** *"Show my unpaid customer invoices from the last 30 days."* → Claude calls `getCustomerInvoices` with a filter like `[{"field":"status","operator":"eq","value":"unpaid"},{"field":"date","operator":"gteq","value":"2026-04-06"}]`, walks the `next_cursor` pagination if needed.

**2. Reconcile a bank transaction.** *"Find an unmatched €1,250 transaction this week and link it to the right invoice."* → Claude calls `getTransactions`, then `pennylane_search_tools(query="match")` to discover the matching operation, then `pennylane_execute(tool_name="postCustomerInvoiceMatchedTransactions", params={"id": <invoice_id>, "transaction_id": <tx_id>})`.

**3. Generate a FEC export for Q1 2026.** *"Generate the FEC for January through March 2026 and tell me when it's ready."* → Claude calls `exportFec` with `{"start_date":"2026-01-01","end_date":"2026-03-31"}`, polls `getFecExport` until status flips to `done`, returns the download URL.

## Read-only mode

`mcp-pennylane` resolves the read-only posture in two steps:

1. **Auto-detect** (default) — when `PENNYLANE_READONLY` is unset, the server probes `GET /me` once at startup, reads the token's `scopes` array, and forces read-only mode iff every scope ends with `:readonly`. Banner reads `mode=readonly (auto)` or `mode=read+write (auto)`.
2. **Explicit override** — set `PENNYLANE_READONLY=true` (or `false`) to skip the probe entirely. Banner reads `mode=… (explicit)`. Useful in CI for deterministic startup.

Whatever the source, when read-only is active, write operations are filtered out at registration time AND `pennylane_execute` returns a structured `READONLY_MODE` error before any HTTP call to Pennylane.

```bash
# Defense-in-depth: explicit + read-only Pennylane token scope
export PENNYLANE_API_KEY=…                # token created with "Read only — retrieve data"
export PENNYLANE_READONLY=true            # explicit server-side filter
mcp-pennylane
```

The Pennylane token scope is the radio button at token creation in **Settings → Connectivity → Developers**. One layer protects against a misconfigured server, the other against a misconfigured token.

## Logging & privacy

- Logs go to **stderr only** — never stdout, which would corrupt the MCP framing on the stdio transport.
- The Pennylane bearer token is **always redacted** through `redact_bearer()` (`Bearer abcd***wxyz`), even at TRACE level.
- INFO / WARN / ERROR logs **never include Pennylane response bodies** — accounting data is sensitive (RGPD, secret des affaires).
- TRACE level surfaces full request URLs and response bodies. Enable it only for local debugging on a sandbox account.
- No telemetry, no phone-home, no opt-in instrumentation.

## Streamable HTTP transport

```bash
mcp-pennylane --transport http --host 127.0.0.1 --port 8000
# MCP endpoint: http://127.0.0.1:8000/mcp
# Health:       http://127.0.0.1:8000/health
```

The Pennylane API key still lives in `PENNYLANE_API_KEY` server-side — the server is single-tenant per process. **Authentication between the MCP client and this server is intentionally none.** Bind to `127.0.0.1` and put a reverse proxy in front of it for any non-local exposure (Cloudflare Access, Authelia, Caddy basic-auth, etc.). An OSS standalone server should not bake in opinions about external auth.

## Sandbox

`PENNYLANE_ENV=sandbox` is a **visual hint only** — Pennylane uses the same base URL for sandbox and production. The value is surfaced in three places to prevent the classic "I just modified production while testing" mistake:

1. The startup banner on stderr: `mcp-pennylane v0.1.0 — Pennylane Company API v2.0 — env=sandbox — mode=readonly (auto)`.
2. The `getMe` response, augmented with `_mcp_pennylane = { env, server_version, spec_version, readonly, readonly_source, api_2026 }`.
3. The MCP `serverInfo.instructions` string.

## API 2026 changes

Pennylane is rolling out breaking changes to its Company API on **April 8, 2026**. From January 14 to April 8, 2026 the new behaviour is opt-in via `X-Use-2026-API-Changes: true`. Set `PENNYLANE_API_2026=true` (or `--api-2026`) to send the header now.

| Phase | When | Behaviour |
|---|---|---|
| Preview | Jan 14 → Apr 8, 2026 | Opt-in via env var |
| Default flip | Apr 8, 2026 | New behaviour upstream-default. **v1.0** tag here, env var becomes opt-out |
| Cleanup | Jul 1, 2026 | Legacy behaviour removed upstream — env var is a no-op |

## Tool catalog

About 73 essentials are exposed directly: customers, suppliers, customer + supplier invoices, products, quotes, banking, journals, ledger accounts/entries/lines, trial balance, fiscal years, analytical categories, FEC exports, file attachments, changelogs, GoCardless and SEPA mandates, `getMe`. The full ~163 operations stay reachable through the meta-tools.

Tool names match the Pennylane `operationId` verbatim (e.g. `getCustomerInvoices`, `postLedgerEntries`), so they grep cleanly against the official OpenAPI spec. The full curated list lives in the [`ESSENTIALS` constant in `server/build.rs`](./server/build.rs).

## Roadmap

- Pennylane Firm API as a sibling binary `mcp-pennylane-firm`
- Docker image once HTTP-mode usage warrants it

## Development

```bash
# Build
cargo build --release

# Refresh the vendored Pennylane OpenAPI spec
cargo run -p refresh-openapi
cargo run -p refresh-openapi -- --diff   # dry run
cargo run -p refresh-openapi -- --check  # exit non-zero if drift

# Run the test suite (unit + invariant)
cargo test --workspace

# Run the integration tests against a Pennylane sandbox (read-only)
PENNYLANE_API_KEY=… PENNYLANE_READONLY=true cargo test --workspace --features integration-tests
```

A weekly GitHub Action runs `cargo run -p refresh-openapi` every Monday at 06:00 UTC and opens a PR if the upstream spec drifted. CI on the PR validates that every essential operation is still present (`build.rs` panics if a renamed/removed op breaks the contract); merge once green.

## License

MIT — see [LICENSE](LICENSE).
