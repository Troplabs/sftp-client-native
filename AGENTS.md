# Repository guidance

## Project shape

This repository builds the @troplabs/sftp-client-native Node.js package as a Rust N-API addon.

- src/lib.rs owns the JavaScript-facing API and adapts it to russh and russh-sftp.
- Cargo.toml and build.rs define the native addon.
- package.json owns package metadata and NAPI-RS build scripts.
- CI runs only on pull requests and pushes to `main`, with read-only repository permissions, and must not require npm credentials.
- The Release workflow publishes to the public npm registry with npm trusted publishing (OIDC) and automatic provenance. Do not change its publish, provenance, or credential handling without an explicit maintainer request.

Keep the first API small and asynchronous. Prefer adding capabilities to the existing client class over exposing russh implementation details to JavaScript.

## Security requirements

- Verify SSH server keys against the user's standard ~/.ssh/known_hosts file, or against caller-supplied hostKeyFingerprint pins when provided, and fail closed for unknown, changed, unmatched, or unreadable keys.
- Never accept every server key, silently add unknown keys, log credentials, or commit real credentials and private keys.
- Keep errors as JavaScript rejections. Do not panic across the N-API boundary.
- File paths are remote SFTP paths. Do not reinterpret them as local filesystem paths.

## Development

Prerequisites: Rust 1.99 or newer (stable channel, per rust-toolchain.toml), Node.js 24.21 or newer, and pnpm 12.

```sh
pnpm install --frozen-lockfile
pnpm run build
cargo fmt --all -- --check
cargo check
```

The NAPI-RS build generates index.mjs, index.d.ts, and a platform-specific .node file. These are build outputs and are gitignored. Run `pnpm run build` before a local `pnpm pack`.

npm distribution uses a thin root package plus one optional dependency package per target. Supported targets are macOS, Linux (glibc), and Windows for both amd64 and arm64.

CI checks formatting and compiles every configured target. The Release workflow gathers those binaries into a draft GitHub Release, stages the platform packages, then stages the root package. Add appropriate coverage as the project gains a test harness.

## Changes

- Update the README when the exported JavaScript API or platform requirements change.
- Keep Cargo and npm metadata aligned at version changes.
- CI must use read-only repository permissions and must not require npm credentials.
- Releases publish from `.github/workflows/release.yml` when a `chore/release-v*` PR merges to `main` (opened by the "Prepare release" workflow) via npm trusted publishing (OIDC) with staged publishing: the workflow runs `npm stage publish` and a maintainer approves each staged version with 2FA on npmjs.com or `npm stage approve`. Trusted publishers allow `npm stage publish` only; direct `npm publish` is rejected. Do not reintroduce long-lived `NPM_TOKEN` secrets.
