# @troplabs/sftp-client-native

An asynchronous Node.js SFTP client backed by the Rust crates [russh](https://github.com/Eugeny/russh) and [russh-sftp](https://github.com/AspectUnk/russh-sftp), exposed through [NAPI-RS](https://napi.rs/).

The client follows the SSH and SFTP flow in russh's [sftp_client.rs example](https://github.com/Eugeny/russh/blob/main/russh/examples/sftp_client.rs). It verifies server keys with the standard OpenSSH known_hosts file. Unknown or changed keys are rejected.

> The package is under initial development. The JavaScript API and native package support may change before the first npm release.

## Requirements

- Node.js 24.21 or newer.
- Rust 1.98 or newer and pnpm 12 to build from source.
- A reachable SSH server with the SFTP subsystem enabled.
- The server's public host key recorded in ~/.ssh/known_hosts.

## Install

After the first public npm release:

```sh
pnpm add @troplabs/sftp-client-native
```

## Usage

Password authentication:

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

Private key authentication (local key path or key material):

```js
const client = await SftpClient.connect({
  host: "sftp.example.com",
  username: process.env.SFTP_USERNAME,
  privateKeyPath: process.env.SFTP_PRIVATE_KEY_PATH,
  // passphrase: process.env.SFTP_KEY_PASSPHRASE, // for encrypted keys
});

// Or pass OpenSSH/PEM key contents directly:
// privateKey: process.env.SFTP_PRIVATE_KEY,
```

SSH agent authentication:

```js
const client = await SftpClient.connect({
  host: "sftp.example.com",
  username: process.env.SFTP_USERNAME,
  agent: true, // uses SSH_AUTH_SOCK on Unix, Pageant on Windows
  // agentSocket: process.env.SSH_AUTH_SOCK, // optional custom socket / named pipe
});
```

Provide exactly one auth mode per connect call. The API is asynchronous. SFTP errors and connection failures reject the returned promises.

### API

- **SftpClient.connect(options)** — connect and authenticate with password, a private key, or an SSH agent.
- **client.readDir(remotePath)** — return the names in a remote directory.
- **client.readFile(remotePath)** — return a remote file as a Node.js Buffer.
- **client.writeFile(remotePath, content)** — create or replace a remote file from a Buffer.
- **client.createDir(remotePath)** — create a remote directory.
- **client.removeFile(remotePath)** and **client.removeDir(remotePath)** — remove a remote file or directory.
- **client.rename(oldRemotePath, newRemotePath)** — rename a remote path.
- **client.close()** — close the SFTP and SSH sessions.

Connection options:

- **host** — SSH server host name or IP address.
- **port** — SSH port; defaults to 22.
- **username** — SSH account name.
- **password** — password for SSH password authentication.
- **privateKey** — OpenSSH/PEM private key material for public key authentication.
- **privateKeyPath** — local filesystem path to an OpenSSH/PEM private key (not a remote SFTP path).
- **passphrase** — passphrase for an encrypted private key; only valid with `privateKey` or `privateKeyPath`.
- **agent** — when `true`, authenticate via the default SSH agent (`SSH_AUTH_SOCK` on Unix, Pageant on Windows).
- **agentSocket** — Unix domain socket or Windows named-pipe path for a specific agent; implies agent auth.

Exactly one auth mode is required: `password`, `privateKey`/`privateKeyPath`, or `agent`/`agentSocket`. `privateKey` and `privateKeyPath` are mutually exclusive.

The client checks ~/.ssh/known_hosts for the host and port. Add the server key through a trusted channel before connecting. It does not use trust-on-first-use behavior. This is intentionally stricter than the upstream example, which accepts any key.

readFile buffers the full remote file in memory. Streaming transfers, custom known-hosts paths, and richer metadata are not part of the initial API.

## Development

```sh
pnpm install --frozen-lockfile
pnpm run build
cargo fmt --all -- --check
cargo check
```

pnpm run build generates the ESM loader, TypeScript declarations, and the native binary for the current platform. Generated build outputs are not committed.

## Continuous integration

GitHub Actions checks formatting and builds the native addon on Linux, macOS, and Windows for pushes and pull requests. Workflows have read-only repository permissions. There is no automated npm publishing or release workflow.

The package's prepack script builds for the current machine. Before the first public release, maintainers should confirm the npm artifact strategy and supported platform targets; a single-machine package archive contains only that machine's native binary.

## License

Apache-2.0. See [LICENSE](LICENSE).
