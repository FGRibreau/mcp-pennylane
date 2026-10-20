#!/usr/bin/env bash
# Point the Homebrew formula at a release: version, four binary URLs, four hashes.
# The hashes are the sha256 digests GitHub records for the release assets.
#
# Usage: GITHUB_REPOSITORY=owner/repo bump-tap-formula.sh [--dry-run] vX.Y.Z FORMULA
# Exit 0: formula rewritten, or already up to date. Exit 1: anything else.
set -euo pipefail

readonly TARGETS=(
  x86_64-apple-darwin
  aarch64-apple-darwin
  x86_64-unknown-linux-gnu
  aarch64-unknown-linux-gnu
)
readonly TAG_PATTERN='^v(0|[1-9][0-9]{0,5})\.(0|[1-9][0-9]{0,5})\.(0|[1-9][0-9]{0,5})$'
readonly REPO_PATTERN='^[A-Za-z0-9._-]{1,100}/[A-Za-z0-9._-]{1,100}$'
readonly DIGEST_PATTERN='^sha256:[0-9a-f]{64}$'
readonly MAX_FORMULA_BYTES=65536
readonly MAX_ASSET_LINES=200
readonly API_TIMEOUT_SECONDS=60

die() {
  echo "bump-tap-formula: $*" >&2
  exit 1
}

# Sets dry_run, tag, version, formula and repo from the command line and environment.
read_inputs() {
  dry_run=false
  if [[ ${1:-} == "--dry-run" ]]; then
    dry_run=true
    shift
  fi
  [[ $# -eq 2 ]] || die "usage: GITHUB_REPOSITORY=owner/repo $0 [--dry-run] vX.Y.Z FORMULA"
  tag=$1
  formula=$2
  repo=${GITHUB_REPOSITORY:-}
  [[ $tag =~ $TAG_PATTERN ]] || die "the tag must look like v1.2.3, got '${tag:0:40}'"
  [[ $repo =~ $REPO_PATTERN ]] || die "GITHUB_REPOSITORY must be owner/repo, got '${repo:0:80}'"
  [[ -f $formula ]] || die "formula file not found: ${formula:0:200}"
  [[ $(wc -c <"$formula") -le $MAX_FORMULA_BYTES ]] || die "formula is larger than $MAX_FORMULA_BYTES bytes"
  command -v gh >/dev/null || die "the gh CLI is required"
  command -v timeout >/dev/null || die "timeout is required"
  version=${tag#v}
}

# Prints "target=hex" for every target, read from the release asset digests.
release_hashes() {
  local assets target name row digest
  if ! assets=$(timeout "$API_TIMEOUT_SECONDS" gh api "repos/${repo}/releases/tags/${tag}" \
    --jq '.assets[] | [.name,.digest] | @tsv'); then
    die "cannot read release ${tag} of ${repo}"
  fi
  [[ $(wc -l <<<"$assets") -le $MAX_ASSET_LINES ]] || die "release ${tag} lists more than $MAX_ASSET_LINES assets"
  for target in "${TARGETS[@]}"; do
    name="mcp-pennylane-${target}.tar.gz"
    row=$(awk -F '\t' -v name="$name" '$1 == name { print "row\t" $2 }' <<<"$assets")
    [[ -n $row ]] || die "release ${tag} has no asset ${name}"
    [[ $row != *$'\n'* ]] || die "release ${tag} has several assets named ${name}"
    digest=${row#row$'\t'}
    [[ $digest =~ $DIGEST_PATTERN ]] || die "asset ${name} has no sha256 digest (got '${digest:0:80}')"
    echo "${target}=${digest#sha256:}"
  done
}

# Copies the formula to stdout with the new version, URLs and hashes.
# Fails unless it finds one version line, and one URL plus one hash per target.
rewrite_formula() {
  awk -v version="$version" \
    -v prefix="https://github.com/${repo}/releases/download/" \
    -v map="$1" '
    function fail(message) {
      print "bump-tap-formula: " message > "/dev/stderr"
      failed = 1
    }
    function indent() {
      match($0, /^ */)
      return substr($0, 1, RLENGTH)
    }
    BEGIN {
      count = split(map, pairs, " ")
      for (i = 1; i <= count; i++) {
        split(pairs[i], kv, "=")
        sha[kv[1]] = kv[2]
      }
    }
    pending != "" {
      if ($0 ~ /^ *sha256 "[0-9a-f]*"$/) {
        print indent() "sha256 \"" sha[pending] "\""
        hashes[pending]++
        pending = ""
        next
      }
      broken++
      pending = ""
    }
    /^ *version "[^"]*"$/ {
      print indent() "version \"" version "\""
      versions++
      next
    }
    {
      head = "url \"" prefix "v"
      pos = index($0, head)
      if (pos > 0 && substr($0, 1, pos - 1) ~ /^ *$/) {
        rest = substr($0, pos + length(head))
        asset = substr(rest, index(rest, "/") + 1)
        for (t in sha) {
          if (asset == "mcp-pennylane-" t ".tar.gz\"") {
            print substr($0, 1, pos - 1) head version "/" asset
            pending = t
            urls[t]++
            next
          }
        }
      }
      if ($0 ~ /^ *sha256 /) stray++
      print
    }
    END {
      if (pending != "") broken++
      if (versions != 1) fail("expected one version line, found " versions + 0)
      for (t in sha) {
        if (urls[t] != 1) fail("expected one url for " t ", found " urls[t] + 0)
        if (hashes[t] != 1) fail("no sha256 line follows the url for " t)
      }
      if (broken > 0 || stray > 0) fail("found a sha256 line that does not follow a release url")
      if (failed) exit 1
    }
  ' "$formula"
}

main() {
  local hashes
  read_inputs "$@"
  hashes=$(release_hashes | tr '\n' ' ')
  updated=$(mktemp "${TMPDIR:-/tmp}/bump-tap-formula.XXXXXX")
  trap 'rm -f "$updated"' EXIT
  rewrite_formula "$hashes" >"$updated" || die "the formula does not have the expected shape"
  if cmp -s "$formula" "$updated"; then
    echo "bump-tap-formula: ${formula} already points at ${tag}, nothing to do"
    return 0
  fi
  if [[ $dry_run == true ]]; then
    diff -u -L "$formula" -L "$formula (${tag})" "$formula" "$updated" || [[ $? -eq 1 ]]
    return 0
  fi
  cat "$updated" >"$formula"
  echo "bump-tap-formula: ${formula} now points at ${tag} (${#TARGETS[@]} urls, ${#TARGETS[@]} sha256)"
}

main "$@"
