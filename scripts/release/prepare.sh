#!/usr/bin/env bash
# Prepare a release: bump versions, push a chore/release-v* branch, open the PR.
#
# Invoked by the "Prepare release" workflow (.github/workflows/release-prepare.yml,
# job `prepare`) after checkout and pnpm/Node/Rust toolchain setup. Also runnable
# locally — the main-branch guard is skipped when GITHUB_REF is unset and step
# summary writes go to /dev/null — but note it pushes a real branch to origin
# and opens a real pull request.
#
# What this does:
#   1. Validates INPUT_VERSION: strips a leading v, must be MAJOR.MINOR.PATCH and
#      strictly greater than the current package.json version.
#   2. Fails if tag vX.Y.Z or branch chore/release-vX.Y.Z already exists on origin.
#   3. Bumps package.json (+ optionalDependencies), Cargo.toml, Cargo.lock, and
#      pnpm-lock.yaml, then verifies every version field agrees and that no
#      other tracked file changed.
#   4. Commits as github-actions[bot], pushes the branch, and opens a PR to
#      main — falling back to a compare link when the token cannot create PRs.
#
# Usage:
#   INPUT_VERSION=0.1.3 scripts/release/prepare.sh
#
# Environment:
#   INPUT_VERSION        (required) version to release, e.g. 0.1.3 or v0.1.3
#   GH_TOKEN             token for `gh pr create` (GITHUB_TOKEN in CI; falls back
#                        to the local `gh` login when unset)
#   GITHUB_REF           CI-only main-branch guard (check skipped when unset)
#   GITHUB_REPOSITORY    owner/name for the manual-PR compare link (inferred
#                        from the origin remote when unset)
#   GITHUB_STEP_SUMMARY  step summary file (writes discarded when unset)
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

# CI guard: 'Prepare release' must be dispatched from main. GITHUB_REF is
# always set in GitHub Actions, so this check is only meaningful there and is
# skipped for local runs.
if [ -n "${GITHUB_REF:-}" ] && [ "$GITHUB_REF" != "refs/heads/main" ]; then
	echo "::error::'Prepare release' must be dispatched from the main branch (ref: $GITHUB_REF)"
	exit 1
fi

if [ -z "${INPUT_VERSION:-}" ]; then
	echo "error: INPUT_VERSION is required (e.g. INPUT_VERSION=0.1.3)" >&2
	exit 1
fi

VERSION="${INPUT_VERSION#v}"
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
	echo "::error::Version '$INPUT_VERSION' must be MAJOR.MINOR.PATCH; prereleases are not supported (npm stage publish would need a --tag)"
	exit 1
fi
CURRENT="$(node -p "require('./package.json').version")"
if ! node -e '
  const [next, current] = process.argv.slice(1).map((v) => v.split(".").map(Number));
  for (let i = 0; i < 3; i++) {
    if (next[i] > current[i]) process.exit(0);
    if (next[i] < current[i]) process.exit(1);
  }
  process.exit(1);
' "$VERSION" "$CURRENT"; then
	echo "::error::Version $VERSION must be strictly greater than the current version $CURRENT"
	exit 1
fi
echo "Version bump: $CURRENT -> $VERSION"

TAG="v$VERSION"
BRANCH="chore/release-$TAG"
if [ -n "$(git ls-remote --tags origin "refs/tags/$TAG")" ]; then
	echo "::error::Tag $TAG already exists on origin"
	exit 1
fi
if [ -n "$(git ls-remote --heads origin "refs/heads/$BRANCH")" ]; then
	echo "::error::Branch $BRANCH already exists on origin"
	exit 1
fi

# Bump every version field to $VERSION.
jq --arg v "$VERSION" '
  .version = $v
  | .optionalDependencies |= with_entries(
      if .key | startswith("@troplabs/sftp-client-native-") then .value = $v else . end
    )
' package.json >package.json.tmp
mv package.json.tmp package.json
# Rewrite only the first `version = "…"` line in Cargo.toml (the package
# version, not a dependency's). awk instead of GNU sed's `0,/addr/` so the
# script also works with BSD userlands (macOS) for local runs.
awk -v version="$VERSION" '
  !done && /^version = / { sub(/"[^"]+"/, "\"" version "\""); done = 1 }
  { print }
' Cargo.toml >Cargo.toml.tmp
mv Cargo.toml.tmp Cargo.toml
cargo update --workspace
# The bumped @troplabs/sftp-client-native-* versions do not exist on npm
# yet, so pnpm drops them from the lockfile as unresolvable optional
# dependencies. That is required: --frozen-lockfile in CI compares
# package.json specifiers against the lockfile. CI=true makes plain
# `pnpm install` run as --frozen-lockfile, so opt out explicitly.
pnpm install --no-frozen-lockfile
# Assert the lockfile is now consistent with the bumped specifiers.
pnpm install --frozen-lockfile
pnpm exec dprint fmt package.json Cargo.toml

# Verify every version field landed and nothing else moved.
node -e '
  const pkg = require("./package.json");
  const version = process.argv[1];
  if (pkg.version !== version) {
    console.error("::error::package.json version is " + pkg.version + ", expected " + version);
    process.exit(1);
  }
  for (const [name, dep] of Object.entries(pkg.optionalDependencies || {})) {
    if (name.startsWith("@troplabs/sftp-client-native-") && dep !== version) {
      console.error("::error::optionalDependency " + name + " is " + dep + ", expected " + version);
      process.exit(1);
    }
  }
' "$VERSION"
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
unexpected="$(git status --porcelain --untracked-files=no | awk '{print $2}' | grep -vxE 'package.json|Cargo.toml|Cargo.lock|pnpm-lock.yaml' || true)"
if [ -n "$unexpected" ]; then
	echo "::error::Unexpected modified files: $unexpected"
	git status --porcelain
	exit 1
fi
git status --porcelain
git diff --stat

# Commit and push the release branch.
git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git checkout -b "$BRANCH"
git add package.json Cargo.toml Cargo.lock pnpm-lock.yaml
git commit -m "chore(release): bump version to v$VERSION"
git push origin "$BRANCH"

# Open the release PR; fall back to a compare link when GITHUB_TOKEN is not
# allowed to create pull requests. Summary writes go to /dev/null outside CI.
repo="${GITHUB_REPOSITORY:-$(git remote get-url origin | sed -E 's#^git@github\.com:##; s#^https://github\.com/##; s#\.git$##')}"
summary="${GITHUB_STEP_SUMMARY:-/dev/null}"
body="$(cat <<EOF
Merging this PR triggers the Release workflow, which:

- tags \`v$VERSION\` on the merge commit,
- creates a draft GitHub Release with the .node binaries and SHA-256 checksums, and
- stages the npm packages via trusted publishing for maintainer approval with 2FA.

Workflows do not run on PRs created by GITHUB_TOKEN — close and reopen this PR to trigger CI checks.
EOF
)"
if pr_out="$(gh pr create --base main --head "$BRANCH" --title "chore(release): v$VERSION" --body "$body" 2>&1)"; then
	echo "$pr_out"
	{
		echo "### Release pull request opened"
		echo ""
		echo "$pr_out"
	} >>"$summary"
else
	echo "::warning::Could not open the pull request automatically (enable Settings → Actions → General → 'Allow GitHub Actions to create and approve pull requests'): $pr_out"
	{
		echo "### Open the release pull request manually"
		echo ""
		echo "GitHub Actions is not allowed to create pull requests in this repository. Enable it under Settings → Actions → General → 'Allow GitHub Actions to create and approve pull requests', or open the PR yourself:"
		echo ""
		echo "https://github.com/$repo/compare/main...$BRANCH?expand=1"
	} >>"$summary"
fi
