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

npm releases are published by `.github/workflows/release.yml` when a `chore/release-v*` pull request is merged to `main`. Publishing uses [npm trusted publishing](https://docs.npmjs.com/trusted-publishers/) (GitHub Actions OIDC) with automatic provenance. There is no long-lived `NPM_TOKEN`.

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

1. Run the "Prepare release" workflow (Actions → Prepare release → Run workflow, enter a version like `0.1.3`). It bumps `package.json`, `Cargo.toml`, `Cargo.lock`, and `pnpm-lock.yaml`, pushes a `chore/release-vX.Y.Z` branch, and opens a PR to `main`. If the run cannot create the PR (Settings → Actions → General → "Allow GitHub Actions to create and approve pull requests" is off), its summary links a compare page to open the PR manually.
2. Close and reopen the PR once to trigger CI checks — workflows do not run on PRs created by `GITHUB_TOKEN` — then review and merge it.
3. Merging runs `release.yml`: it tags `vX.Y.Z` on the merge commit, creates a draft GitHub Release with the `.node` binaries and SHA-256 checksums, and stages the npm packages with `npm stage publish`.
4. Approve each staged version with 2FA on npmjs.com (Staged Packages tab) or `npm stage approve <stage-id>` — the six platform packages before the root package — then publish the draft GitHub Release.

Do not add alternate publish paths or long-lived npm write tokens in pull requests without maintainer review.
