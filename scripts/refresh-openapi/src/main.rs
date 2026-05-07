//! Refresh the vendored Pennylane Company API v2 OpenAPI spec.
//!
//! Downloads `accounting.json` from Pennylane's docs portal, validates that
//! the file is a usable OpenAPI 3.x document, and overwrites
//! `openapi/accounting.json` if the content changed.
//!
//! Usage:
//!     cargo run -p refresh-openapi              # download and overwrite if changed
//!     cargo run -p refresh-openapi -- --check   # exit non-zero if the vendored copy is stale
//!     cargo run -p refresh-openapi -- --diff    # print a per-path diff without writing

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use indexmap::IndexMap;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::io::Read;
use std::path::PathBuf;

const SPEC_URL: &str = "https://pennylane.readme.io/openapi/accounting.json";
const VENDORED_RELATIVE: &str = "openapi/accounting.json";

#[derive(Parser, Debug)]
#[command(
    name = "refresh-openapi",
    about = "Refresh the vendored Pennylane Company API v2 OpenAPI spec"
)]
struct Args {
    /// Exit with non-zero status if the vendored copy differs from upstream.
    #[arg(long)]
    check: bool,

    /// Print the diff (added/removed/method-changed paths) without writing.
    #[arg(long)]
    diff: bool,

    /// Override the upstream URL.
    #[arg(long, default_value = SPEC_URL)]
    url: String,
}

#[derive(Debug, Deserialize)]
struct MinimalSpec {
    openapi: String,
    paths: IndexMap<String, IndexMap<String, serde_json::Value>>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let workspace_root = locate_workspace_root()?;
    let vendored_path = workspace_root.join(VENDORED_RELATIVE);

    eprintln!("→ downloading {}", args.url);
    let body = download(&args.url)?;
    eprintln!("  ✓ {} bytes", body.len());

    let upstream: MinimalSpec = serde_json::from_slice(&body)
        .with_context(|| format!("upstream {} is not valid JSON", args.url))?;
    if !upstream.openapi.starts_with("3.") {
        bail!(
            "upstream OpenAPI version `{}` is not 3.x — refusing to vendor",
            upstream.openapi
        );
    }
    let upstream_paths: BTreeSet<&str> = upstream.paths.keys().map(|s| s.as_str()).collect();
    eprintln!(
        "  ✓ OpenAPI {} with {} paths",
        upstream.openapi,
        upstream_paths.len()
    );

    let vendored_bytes = std::fs::read(&vendored_path)
        .with_context(|| format!("failed to read vendored spec {}", vendored_path.display()))?;
    let vendored: MinimalSpec = serde_json::from_slice(&vendored_bytes).with_context(|| {
        format!(
            "vendored spec {} is not valid JSON",
            vendored_path.display()
        )
    })?;
    let vendored_paths: BTreeSet<&str> = vendored.paths.keys().map(|s| s.as_str()).collect();

    let added: Vec<&&str> = upstream_paths.difference(&vendored_paths).collect();
    let removed: Vec<&&str> = vendored_paths.difference(&upstream_paths).collect();

    let identical = body == vendored_bytes;
    if identical {
        eprintln!("  ✓ vendored spec is up to date — no action");
        return Ok(());
    }

    if !added.is_empty() {
        eprintln!("\nAdded paths ({}):", added.len());
        for p in &added {
            eprintln!("  + {}", p);
        }
    }
    if !removed.is_empty() {
        eprintln!("\nRemoved paths ({}):", removed.len());
        for p in &removed {
            eprintln!("  - {}", p);
        }
    }
    if added.is_empty() && removed.is_empty() {
        eprintln!("\nSchema-level changes only (no path added or removed).");
    }

    if args.check {
        bail!("vendored spec drift detected — re-run without --check to refresh");
    }
    if args.diff {
        eprintln!("\n--diff: skipping write");
        return Ok(());
    }

    std::fs::write(&vendored_path, &body)
        .with_context(|| format!("failed to write {}", vendored_path.display()))?;
    eprintln!("\n  ✓ refreshed {}", vendored_path.display());

    Ok(())
}

fn download(url: &str) -> Result<Vec<u8>> {
    let response = ureq::get(url)
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .map_err(|e| anyhow!("download failed: {}", e))?;
    if response.status() / 100 != 2 {
        bail!("upstream returned HTTP {}", response.status());
    }
    let mut buf = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut buf)
        .context("failed to read upstream body")?;
    Ok(buf)
}

fn locate_workspace_root() -> Result<PathBuf> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let mut current = PathBuf::from(manifest_dir);
    for _ in 0..5 {
        if current.join(VENDORED_RELATIVE).exists() {
            return Ok(current);
        }
        if !current.pop() {
            break;
        }
    }
    bail!(
        "failed to locate workspace root containing {} (started from {})",
        VENDORED_RELATIVE,
        manifest_dir
    )
}
