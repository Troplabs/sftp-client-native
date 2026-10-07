# @troplabs/sftp-client-native

An asynchronous Node.js SFTP client backed by the Rust crates [russh](https://github.com/Eugeny/russh) and [russh-sftp](https://github.com/AspectUnk/russh-sftp), exposed through [NAPI-RS](https://napi.rs/).

Use it when you need a small, promise-based SFTP API in Node.js without depending on OpenSSH CLI wrappers or pure-JS SSH stacks. The client verifies server host keys against the standard OpenSSH `~/.ssh/known_hosts` file — or against caller-supplied fingerprint pins — and fails closed for unknown, changed, or unmatched keys.

> The package is under initial development. The JavaScript API may change before 1.0.

## Requirements

- Node.js 24.21 or newer.
- Rust 1.99 or newer (stable channel) and pnpm 12 to build from source.
- A reachable SSH server with the SFTP subsystem enabled.
- The server's public host key recorded in `~/.ssh/known_hosts`, unless `hostKeyFingerprint` pins are supplied at connect time.

## Install

```sh
pnpm add @troplabs/sftp-client-native
```

Prebuilt native binaries are published for:

| OS      | Architectures            |
| ------- | ------------------------ |
| macOS   | amd64, arm64 (macOS 26+) |
| Linux   | amd64, arm64 (glibc)     |
| Windows | amd64, arm64             |

Install pulls the matching optional platform package automatically. Building from source still works for local development:

```sh
pnpm install --frozen-lockfile
pnpm run build
```

Import the package from your application after install, or after a local build produces `index.mjs`, `index.d.ts`, and the platform `.node` binary.

## Quick start

```js
import { SftpClient } from "@troplabs/sftp-client-native";

async function main() {
  const client = await SftpClient.connect({
    host: "sftp.example.com",
    port: 22,
    username: process.env.SFTP_USERNAME,
    password: process.env.SFTP_PASSWORD,
  });

  try {
    const names = await client.readDir("/incoming");
    console.log(names);

    const content = await client.readFile("/incoming/report.csv");
    await client.writeFile("/outgoing/report-copy.csv", content);
    await client.createDir("/outgoing/archive");
    await client.rename("/outgoing/report-copy.csv", "/outgoing/archive/report.csv");
  } finally {
    await client.close();
  }
}

main().catch((error) => {
  console.error("SFTP operation failed:", error);
  process.exitCode = 1;
});
```

Always call `close()` when finished. Prefer `try` / `finally` so the SSH session is torn down even when an operation rejects.

## Host key verification

Before `connect` returns a client, the TCP connection completes an SSH handshake and the client checks the server key against `~/.ssh/known_hosts` for the host and port.

- Unknown, changed, or unreadable keys are rejected.
- There is no trust-on-first-use (TOFU) behavior and no automatic writing of new host keys.
- Add the server key through a trusted channel first (for example `ssh-keyscan` reviewed by an operator, or your infra provisioning flow).

This is intentionally stricter than russh's upstream SFTP example, which accepts any server key.

### Pinning a host key fingerprint

Pass `hostKeyFingerprint` in `ConnectOptions` to verify the presented server key against one or more OpenSSH `SHA256:` / `SHA512:` fingerprints instead of `known_hosts`. A pin accepts a single string or an array; any matching entry passes.

```js
const client = await SftpClient.connect({
  host: "sftp.example.com",
  username: process.env.SFTP_USERNAME,
  password: process.env.SFTP_PASSWORD,
  hostKeyFingerprint: process.env.SFTP_HOST_KEY_FINGERPRINT, // e.g. "SHA256:..."
  // or: hostKeyFingerprint: ["SHA256:...", "SHA512:..."],
});
```

- A pin **replaces** the `known_hosts` check entirely — `~/.ssh/known_hosts` is not read, so this works in containers and CI without a populated file.
- Servers can present different host key types (ed25519, RSA, ECDSA) depending on negotiation. Pin every key type the server may present, or connections may fail when a different type is negotiated.
- `SHA256:` and `SHA512:` prefixes only; MD5 fingerprints are rejected before connecting.

Get a server's fingerprints from a trusted channel:

```sh
# over the network — review against an out-of-band source before trusting
ssh-keyscan -p 22 sftp.example.com | ssh-keygen -lf -

# on the server itself, for each host key type
ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub
```

`hostKeyFingerprint` accepts the same `SHA256:`/`SHA512:` strings `ssh-keygen -lf` prints (trailing `=` padding is tolerated). After connecting, `client.hostKeyFingerprint` returns the `SHA256:` fingerprint of the key the server actually presented.

## Authentication

`SftpClient.connect(options)` requires **exactly one** auth mode:

| Mode                | Options                                                            |
| ------------------- | ------------------------------------------------------------------ |
| Password            | `password`                                                         |
| Public key          | `privateKey` **or** `privateKeyPath` (optional `passphrase`)       |
| OpenSSH certificate | public-key options **plus** `certificate` **or** `certificatePath` |
| SSH agent           | `agent: true` and/or `agentSocket`                                 |

Do not combine modes (for example password + private key). `privateKey` and `privateKeyPath` are mutually exclusive; so are `certificate` and `certificatePath`.

Store secrets in environment variables or a secret manager. Never commit real passwords, private keys, or certificates.

### Password

```js
const client = await SftpClient.connect({
  host: "sftp.example.com",
  username: process.env.SFTP_USERNAME,
  password: process.env.SFTP_PASSWORD,
});
```

### Private key

Pass either a local filesystem path or the key material as a string (OpenSSH or PEM). Paths are local machine paths, not remote SFTP paths.

```js
const client = await SftpClient.connect({
  host: "sftp.example.com",
  username: process.env.SFTP_USERNAME,
  privateKeyPath: process.env.SFTP_PRIVATE_KEY_PATH,
  // passphrase: process.env.SFTP_KEY_PASSPHRASE, // encrypted keys only
});
```

```js
const client = await SftpClient.connect({
  host: "sftp.example.com",
  username: process.env.SFTP_USERNAME,
  privateKey: process.env.SFTP_PRIVATE_KEY,
});
```

### OpenSSH certificate

Certificate authentication uses a user certificate together with the matching private key (the same pairing OpenSSH uses with `IdentityFile` + `CertificateFile`).

```js
const client = await SftpClient.connect({
  host: "sftp.example.com",
  username: process.env.SFTP_USERNAME,
  privateKeyPath: process.env.SFTP_PRIVATE_KEY_PATH,
  certificatePath: process.env.SFTP_CERTIFICATE_PATH,
});
```

```js
const client = await SftpClient.connect({
  host: "sftp.example.com",
  username: process.env.SFTP_USERNAME,
  privateKey: process.env.SFTP_PRIVATE_KEY,
  certificate: process.env.SFTP_CERTIFICATE,
  // passphrase: process.env.SFTP_KEY_PASSPHRASE,
});
```

`certificate` / `certificatePath` without a private key is rejected. For certificates loaded in an SSH agent, use agent auth instead.

### SSH agent

```js
const client = await SftpClient.connect({
  host: "sftp.example.com",
  username: process.env.SFTP_USERNAME,
  agent: true,
});
```

- On Unix, `agent: true` uses `SSH_AUTH_SOCK`.
- On Windows, `agent: true` uses Pageant when available.
- Set `agentSocket` to a Unix domain socket path or Windows named-pipe path to target a specific agent. Providing `agentSocket` alone implies agent auth.

The client requests agent identities and tries each public key and OpenSSH certificate until one succeeds.

```js
const client = await SftpClient.connect({
  host: "sftp.example.com",
  username: process.env.SFTP_USERNAME,
  agentSocket: process.env.SSH_AUTH_SOCK,
});
```

## File operations

Remote paths are SFTP paths on the server. They are not interpreted as local filesystem paths.

| Method                                 | Behavior                                           |
| -------------------------------------- | -------------------------------------------------- |
| `readDir(remotePath)`                  | Returns entry names in a remote directory          |
| `readFile(remotePath)`                 | Returns the full remote file as a Node.js `Buffer` |
| `writeFile(remotePath, content)`       | Creates or replaces a remote file from a `Buffer`  |
| `createDir(remotePath)`                | Creates a remote directory                         |
| `removeFile(remotePath)`               | Removes a remote file                              |
| `removeDir(remotePath)`                | Removes a remote directory                         |
| `rename(oldRemotePath, newRemotePath)` | Renames a remote path                              |
| `close()`                              | Closes the SFTP subsystem and disconnects SSH      |

`readFile` buffers the entire remote file in memory. For large files, plan memory accordingly; streaming transfers are not part of the current API.

Operations on a closed client reject with an error indicating the client is closed. Calling `close()` more than once is safe.

## Errors

Connection, authentication, and SFTP failures reject the returned promises. Catch at the call site or with `.catch` on your top-level async entry point.

Typical failure cases:

- Host key missing or mismatched in `known_hosts`, or not matching `hostKeyFingerprint`
- Auth rejected (wrong password, key, certificate, or agent identity)
- Network / DNS failures
- Remote path not found or permission denied

Host-key rejection errors include the presented key's algorithm and `SHA256:` fingerprint so the actual server key can be identified.

The library does not log credentials, private keys, or certificate material.

## API reference

### `SftpClient.connect(options): Promise<SftpClient>`

Connects, verifies the host key, authenticates, and opens the SFTP subsystem.

#### Connection fields

- **host** `string` — SSH server host name or IP address.
- **port** `number` (optional) — SSH port; defaults to `22`.
- **username** `string` — SSH account name.
- **hostKeyFingerprint** `string | string[]` (optional) — OpenSSH `SHA256:`/`SHA512:` fingerprint(s) of the expected server host key. When set, this pin replaces the `~/.ssh/known_hosts` check; the presented key must match at least one entry. See [Pinning a host key fingerprint](#pinning-a-host-key-fingerprint).

#### Auth fields

- **password** `string` (optional) — password authentication.
- **privateKey** `string` (optional) — OpenSSH/PEM private key material.
- **privateKeyPath** `string` (optional) — local path to an OpenSSH/PEM private key.
- **passphrase** `string` (optional) — passphrase for an encrypted private key; only valid with `privateKey` or `privateKeyPath`.
- **certificate** `string` (optional) — OpenSSH user certificate material; requires a private key.
- **certificatePath** `string` (optional) — local path to an OpenSSH user certificate; requires a private key.
- **agent** `boolean` (optional) — when `true`, authenticate via the default SSH agent.
- **agentSocket** `string` (optional) — agent socket / named-pipe path; implies agent auth.

### Instance properties

- **hostKeyFingerprint** `string` (read-only) — `SHA256:` fingerprint of the server host key verified during `connect`. Remains available after `close()`.

### Instance methods

See [File operations](#file-operations) above.

## Limitations

Not part of the current API:

- Streaming uploads/downloads
- Custom known-hosts file paths
- Keyboard-interactive or GSSAPI authentication
- Auth fallback chains (for example try key, then password)
- Rich remote file metadata (mode, mtime, size beyond what you derive yourself)

## Development

```sh
pnpm install --frozen-lockfile
pnpm run build
cargo fmt --all -- --check
cargo check
```

`pnpm run build` generates the ESM loader, TypeScript declarations, and the native binary for the current platform. Generated build outputs are not committed.

### Smoke test against public SFTP servers

[sftp.net](https://www.sftp.net/public-online-sftp-servers) lists free demo servers useful for a quick password-auth check:

| Host                  | Port | Login                           |
| --------------------- | ---- | ------------------------------- |
| `test.rebex.net`      | 22   | `demo` / `password` (read-only) |
| `demo.wftpserver.com` | 2222 | `demo` / `demo`                 |

Trust each host key first (review `ssh-keyscan` output before appending):

```sh
ssh-keyscan -p 22 test.rebex.net >> ~/.ssh/known_hosts
ssh-keyscan -p 2222 demo.wftpserver.com >> ~/.ssh/known_hosts
pnpm run build
pnpm run smoke:public-sftp
```

This only exercises password auth, `readDir` / `readFile`, and (on the writable demo) a small write round-trip. It does not cover private-key, certificate, or agent auth.

## Continuous integration and releases

GitHub Actions CI runs on pull requests and pushes to `main`. It checks formatting and builds every supported platform target (macOS 26+, Linux glibc, and Windows × amd64/arm64) with read-only repository permissions.

Merging a `chore/release-v*` PR (opened by the "Prepare release" workflow) runs the Release workflow, which rebuilds those targets, attaches the binaries to a draft GitHub Release, and publishes via [npm trusted publishing](https://docs.npmjs.com/trusted-publishers/) (OIDC). That publishes one optional package per platform, then the root `@troplabs/sftp-client-native` package, with automatic provenance and no long-lived npm token. See [CONTRIBUTING.md](CONTRIBUTING.md) for Trusted Publisher setup.

## License

Apache-2.0. See [LICENSE](LICENSE).
