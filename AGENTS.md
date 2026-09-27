# Repository guidance

## Project shape

This repository builds the @troplabs/sftp-client-native Node.js package as a Rust N-API addon.

- src/lib.rs owns the JavaScript-facing API and adapts it to russh and russh-sftp.
- Cargo.toml and build.rs define the native addon.
- package.json owns package metadata and NAPI-RS build scripts.
- The GitHub Actions workflow is CI only. Never add npm publishing, release creation, or deployment steps unless the maintainers explicitly ask for them.

Keep the first API small and asynchronous. Prefer adding capabilities to the existing client class over exposing russh implementation details to JavaScript.

## Security requirements

- Verify SSH server keys against the user's standard ~/.ssh/known_hosts file and fail closed for unknown, changed, or unreadable keys.
- Never accept every server key, silently add unknown keys, log credentials, or commit real credentials and private keys.
- Keep errors as JavaScript rejections. Do not panic across the N-API boundary.
- File paths are remote SFTP paths. Do not reinterpret them as local filesystem paths.

## Development

Prerequisites: Rust 1.98 or newer, Node.js 24.21 or newer, and pnpm 12.

```sh
pnpm install --frozen-lockfile
pnpm run build
cargo fmt --all -- --check
cargo check
```

The NAPI-RS build generates index.mjs, index.d.ts, and a platform-specific .node file. These are build outputs and are gitignored. pnpm pack runs the local build before creating a package archive.

CI currently checks formatting and compiles the native addon. Add appropriate coverage as the project gains a test harness.

## Changes

- Update the README when the exported JavaScript API or platform requirements change.
- Keep Cargo and npm metadata aligned at version changes.
- CI must use read-only repository permissions and must not require npm credentials.
- Do not add a publishing workflow. A maintainer will handle any npm release manually.
