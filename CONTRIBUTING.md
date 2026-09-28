# Contributing

Thanks for helping improve sftp-client-native.

## Before opening a pull request

- Search existing issues and pull requests for related work.
- Keep changes focused and update the README when the JavaScript API or supported requirements change.
- Never include real passwords, private keys, or production host details in changes, logs, examples, or issue reports.

## Local checks

Install dependencies and build the native addon:

```sh
pnpm install --frozen-lockfile
pnpm run build
cargo fmt --all -- --check
cargo check
```

Builds generate local files such as index.mjs, index.d.ts, and .node binaries. These files are not committed.

## Security reports

Do not report security vulnerabilities in public issues. Use GitHub's private vulnerability reporting for this repository when it is enabled, or contact the maintainers privately through the Troplabs GitHub organization.

## Releases

npm releases are published by `.github/workflows/release.yml` when a `v*` tag is pushed. Publishing uses [npm trusted publishing](https://docs.npmjs.com/trusted-publishers/) (GitHub Actions OIDC) with automatic provenance. There is no long-lived `NPM_TOKEN`.

Before the first release:

1. Ensure the root package and each platform package exist on npmjs.com (create them under the `@troplabs` org, or bootstrap once with a short-lived granular token and revoke it afterward).
2. On npmjs.com, configure a Trusted Publisher for `@troplabs/sftp-client-native` and each platform package (`-darwin-x64`, `-darwin-arm64`, `-linux-x64-gnu`, `-linux-arm64-gnu`, `-win32-x64-msvc`, `-win32-arm64-msvc`):
   - Organization or user: `troplabs`
   - Repository: `sftp-client-native`
   - Workflow filename: `release.yml`
   - Environment name (optional but recommended): `npm` (must match the workflow `environment`)
3. Optionally require reviewers on the GitHub Environment named `npm`.
4. After trusted publishing works, set each package's Publishing access to require 2FA and disallow tokens.

Release steps:

1. Align `version` in `package.json` and `Cargo.toml` / `Cargo.lock`.
2. Push the release commit, then tag it (`git tag v0.1.0 && git push origin v0.1.0`).
3. Confirm the Release workflow published the root package and all six platform packages with provenance.

Do not add alternate publish paths or long-lived npm write tokens in pull requests without maintainer review.
