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

GitHub Actions runs CI only. npm releases are performed manually by maintainers; pull requests must not add automated publishing or release creation.
