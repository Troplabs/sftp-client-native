#!/usr/bin/env bash
# Finalize a release commit: verify versions, tag it, open a draft GitHub Release.
#
# Invoked by the Release workflow (.github/workflows/release.yml, job `prepare`)
# after the "Resolve release ref" step and checkout of the release commit.
# Runnable locally — job outputs go to $GITHUB_OUTPUT (/dev/null when unset) —
# but note it creates and pushes a real tag and creates a real draft release.
#
# What this does:
#   1. Resolves the release commit from HEAD and reads the version from
#      package.json.
#   2. Verifies package.json optionalDependencies, Cargo.toml, and Cargo.lock
#      all carry that version, and that a merged release PR matches it
#      (HEAD_REF == chore/release-vX.Y.Z).
#   3. Writes sha/tag/version to $GITHUB_OUTPUT for the downstream jobs.
#   4. Creates annotated tag vX.Y.Z at that commit and pushes it — an existing
#      tag must already point at the same commit.
#   5. Creates the draft GitHub Release (skipped when it already exists).
#
# Usage:
#   EVENT_NAME=workflow_dispatch scripts/release/finalize.sh
#
# Environment:
#   EVENT_NAME     github.event_name — pull_request | workflow_dispatch
#   HEAD_REF       github.event.pull_request.head.ref (checked when
#                  EVENT_NAME=pull_request)
#   GH_TOKEN       token for `gh release` (GITHUB_TOKEN in CI; falls back to
#                  the local `gh` login when unset)
#   GITHUB_OUTPUT  job outputs file (writes discarded when unset)
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

SHA="$(git rev-parse HEAD)"
VERSION="$(jq -r '.version' package.json)"
TAG="v$VERSION"
echo "Release $TAG at $SHA (event: ${EVENT_NAME:-local})"

mismatched="$(jq -r --arg v "$VERSION" '
  (.optionalDependencies // {})
  | to_entries[]
  | select((.key | startswith("@troplabs/sftp-client-native-")) and (.value != $v))
  | .key + "@" + .value
' package.json)"
if [ -n "$mismatched" ]; then
	echo "::error::optionalDependencies not at $VERSION: $mismatched"
	exit 1
fi
cargo_version="$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)"
if [ "$cargo_version" != "$VERSION" ]; then
	echo "::error::Cargo.toml version is $cargo_version, expected $VERSION"
	exit 1
fi
lock_version="$(awk -F'"' '/^name = "sftp-client-native"$/{f=1} f && /^version = /{print $2; exit}' Cargo.lock)"
if [ "$lock_version" != "$VERSION" ]; then
	echo "::error::Cargo.lock sftp-client-native version is $lock_version, expected $VERSION"
	exit 1
fi
if [ "${EVENT_NAME:-}" = "pull_request" ] && [ "${HEAD_REF:-}" != "chore/release-$TAG" ]; then
	echo "::error::Release branch ${HEAD_REF:-<unset>} does not match package.json version tag $TAG"
	exit 1
fi

{
	echo "sha=$SHA"
	echo "tag=$TAG"
	echo "version=$VERSION"
} >>"${GITHUB_OUTPUT:-/dev/null}"

# A tag pushed with GITHUB_TOKEN does not trigger another run of this
# workflow — that is intended, everything happens in this run.
if [ -n "$(git ls-remote --tags origin "refs/tags/$TAG")" ]; then
	git fetch --quiet origin "refs/tags/$TAG:refs/tags/$TAG"
	tagged_sha="$(git rev-parse "$TAG^{commit}")"
	if [ "$tagged_sha" != "$SHA" ]; then
		echo "::error::Tag $TAG already exists at $tagged_sha, expected $SHA"
		exit 1
	fi
	echo "Tag $TAG already points at $SHA"
else
	git config user.name "github-actions[bot]"
	git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
	git tag -a "$TAG" -m "Release $TAG" "$SHA"
	git push origin "$TAG"
	echo "Pushed tag $TAG at $SHA"
fi

if gh release view "$TAG" >/dev/null 2>&1; then
	echo "Release $TAG already exists; leaving it unchanged"
else
	gh release create "$TAG" --draft --generate-notes --verify-tag --title "$TAG"
fi
