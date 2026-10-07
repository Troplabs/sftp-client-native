#!/usr/bin/env bash
# Stage an npm release locally from CI-built binaries.
#
# Workaround for https://github.com/npm/cli/issues/9969 — npm's OIDC token
# exchange rejects GitHub's immutable `sub` claim format, which is mandatory
# for repositories created after 2026-07-15 (this repo: 2026-09-27). Until npm
# fixes the exchange, the Release workflow cannot authenticate, so the same
# staging step runs here under an interactive `npm login` session instead.
#
# Notes:
#   - Staged tarballs get no provenance attestation — provenance requires the
#     OIDC identity that only exists inside CI (`--no-provenance` below).
#   - Re-runnable: entries already in `npm stage list` are skipped, so a
#     partially completed run can be resumed safely.
#
# What this does:
#   1. Downloads the bindings-* artifacts from a Release workflow run.
#   2. Recreates npm/<platform>/ package dirs and drops in the .node binaries
#      (same commands the publish job uses).
#   3. Runs `npm stage publish --access public` for each platform package,
#      then the root package — staged, not live.
#   4. Prints next steps: approve each staged version with 2FA on npmjs.com
#      (Staged Packages tab) or `npm stage approve <stage-id>`.
#
# Usage:
#   scripts/stage-release.sh [RUN_ID]   # RUN_ID = `gh run` id; defaults to the
#                                       # latest non-skipped Release workflow run
#   DRY_RUN=1 scripts/stage-release.sh  # pack and validate only, no upload
#
# Requires: gh auth, pnpm, and `npm login` with publish rights on @troplabs.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

VERSION="$(node -p "require('./package.json').version")"
DRY_RUN_FLAG=""
if [ "${DRY_RUN:-0}" = "1" ]; then
	DRY_RUN_FLAG="--dry-run"
	echo "DRY RUN — nothing will be uploaded"
fi
echo "Staging @troplabs/sftp-client-native@${VERSION}"

if ! npm whoami >/dev/null 2>&1; then
	echo "error: not logged in to npm — run 'npm login' first." >&2
	exit 1
fi

RUN_ID="${1:-$(gh run list --workflow release.yml --limit 50 --json databaseId,conclusion --jq '[.[] | select(.conclusion != "skipped")][0].databaseId // empty')}"
if [ -z "${RUN_ID}" ]; then
	echo "error: could not resolve a Release workflow run; pass RUN_ID explicitly." >&2
	exit 1
fi
echo "Downloading artifacts from run ${RUN_ID}"
rm -rf artifacts
gh run download "${RUN_ID}" -D artifacts

# Same two commands the publish job runs.
pnpm exec napi create-npm-dirs
pnpm artifacts

# Restore the generated loader from any build artifact (identical across jobs).
restored=0
for dir in artifacts/bindings-*/; do
	if [ -f "${dir}index.mjs" ] && [ -f "${dir}index.d.ts" ]; then
		cp "${dir}index.mjs" "${dir}index.d.ts" .
		echo "Restored index.mjs and index.d.ts from ${dir}"
		restored=1
		break
	fi
done
if [ "${restored}" -ne 1 ]; then
	echo "error: no index.mjs/index.d.ts found in artifacts" >&2
	exit 1
fi

# Verify every platform package has a .node binary at the expected version.
expected=(
	darwin-x64
	darwin-arm64
	linux-x64-gnu
	linux-arm64-gnu
	win32-x64-msvc
	win32-arm64-msvc
)
for platform in "${expected[@]}"; do
	dir="npm/${platform}"
	nodes=("${dir}"/*.node)
	pkg_version="$(node -p "require('./${dir}/package.json').version" 2>/dev/null || echo MISSING)"
	if [ ! -d "${dir}" ] || [ ! -e "${nodes[0]}" ] || [ "${pkg_version}" != "${VERSION}" ]; then
		echo "error: ${dir} incomplete (binary: $(basename "${nodes[0]}" 2>/dev/null || echo none), version: ${pkg_version})" >&2
		exit 1
	fi
	echo "OK ${dir}: $(basename "${nodes[0]}") v${pkg_version}"
done

# Entries already staged are skipped so this script can resume a partial run
# (re-staging the same version is rejected by the registry).
STAGED="$(npm stage list --json 2>/dev/null || echo '[]')"
already_staged() {
	node -e "
    const s = JSON.parse(process.argv[1] || '[]');
    process.exit(s.some(e => e.packageName === process.argv[2] && e.version === process.argv[3]) ? 0 : 1);
  " "${STAGED}" "$1" "${VERSION}"
}

# Platform packages first so the root package's optionalDependencies are
# already staged when it is staged. The trailing slash is required — without
# it npm parses `npm/<dir>` as a GitHub owner/repo package spec.
# --no-provenance: OIDC provenance is unavailable outside CI.
for dir in npm/*/; do
	name="$(node -p "require('./${dir}/package.json').name")"
	if already_staged "${name}"; then
		echo "Already staged: ${name}@${VERSION} — skipping"
		continue
	fi
	echo "Staging platform package ${dir}"
	npm stage publish --access public --no-provenance ${DRY_RUN_FLAG} "${dir}"
done

ROOT_NAME="$(node -p "require('./package.json').name")"
if already_staged "${ROOT_NAME}"; then
	echo "Already staged: ${ROOT_NAME}@${VERSION} — skipping"
else
	echo "Staging root package"
	npm stage publish --access public --no-provenance ${DRY_RUN_FLAG}
fi

rm -rf artifacts

if [ -z "${DRY_RUN_FLAG}" ]; then
	npm stage list 2>/dev/null || true
	cat <<EOF

All packages are staged but NOT yet live. Approve each staged version with 2FA:
  - npmjs.com → package → Staged Packages tab, or
  - npm stage approve <stage-id>   (see 'npm stage list' output above)

Approve the six platform packages before @troplabs/sftp-client-native so its
optionalDependencies resolve on the first install.

Verify after approval:
  npm view @troplabs/sftp-client-native@${VERSION}
EOF
fi
