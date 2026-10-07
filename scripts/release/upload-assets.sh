#!/usr/bin/env bash
# Attach the six platform .node binaries + SHA-256 checksums to a GitHub Release.
#
# Invoked by the Release workflow (.github/workflows/release.yml, job
# `github-release`) after actions/download-artifact has merged the bindings-*
# artifacts into dist/. Runnable locally against a hand-populated dist/.
#
# What this does:
#   1. Verifies dist/ contains exactly the six expected
#      sftp-client-native.<platform>.node binaries (missing or unexpected
#      binaries fail the run; other files such as index.mjs are ignored).
#   2. Writes dist/<file>.sha256 for each binary.
#   3. Uploads the binaries and checksums to release TAG with --clobber.
#
# Usage:
#   scripts/release/upload-assets.sh v0.1.3   # positional arg wins over $TAG
#   TAG=v0.1.3 scripts/release/upload-assets.sh
#
# Environment:
#   TAG                release tag (required — positional arg or env var)
#   GH_TOKEN           token for `gh release upload` (GITHUB_TOKEN in CI; falls
#                      back to the local `gh` login when unset)
#   GITHUB_REPOSITORY  owner/name for --repo (inferred from the local checkout
#                      when unset)
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

TAG="${1:-${TAG:-}}"
if [ -z "$TAG" ]; then
	echo "error: a release tag is required (usage: scripts/release/upload-assets.sh v0.1.3)" >&2
	exit 1
fi

if [ ! -d dist ]; then
	echo "::error::dist/ does not exist — run actions/download-artifact (or populate dist/) first"
	exit 1
fi

expected=(
	darwin-x64
	darwin-arm64
	linux-x64-gnu
	linux-arm64-gnu
	win32-x64-msvc
	win32-arm64-msvc
)
missing=0
for platform in "${expected[@]}"; do
	file="dist/sftp-client-native.${platform}.node"
	if [ ! -f "$file" ]; then
		echo "::error::Missing $file"
		missing=1
	fi
done
count="$(find dist -maxdepth 1 -name '*.node' | wc -l)"
if [ "$count" -ne "${#expected[@]}" ]; then
	echo "::error::Expected ${#expected[@]} .node binaries in dist, found $count"
	missing=1
fi
if [ "$missing" -ne 0 ]; then
	ls -la dist
	exit 1
fi

(
	cd dist
	for f in *.node; do
		if command -v sha256sum >/dev/null 2>&1; then
			sha256sum "$f" >"$f.sha256"
		else
			shasum -a 256 "$f" >"$f.sha256" # macOS fallback for local runs
		fi
	done
)

if [ -n "${GITHUB_REPOSITORY:-}" ]; then
	gh release upload "$TAG" dist/*.node dist/*.sha256 --clobber --repo "$GITHUB_REPOSITORY"
else
	# Outside CI: let gh resolve the repo from the origin remote.
	gh release upload "$TAG" dist/*.node dist/*.sha256 --clobber
fi
