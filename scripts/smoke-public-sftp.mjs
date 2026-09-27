/**
 * Smoke-test @troplabs/sftp-client-native against public demo SFTP servers.
 *
 * Servers listed at https://www.sftp.net/public-online-sftp-servers
 *
 * Prerequisites:
 *   1. pnpm run build
 *   2. Trust each host key once (review output before accepting):
 *        ssh-keyscan -p 22 test.rebex.net >> ~/.ssh/known_hosts
 *        ssh-keyscan -p 2222 demo.wftpserver.com >> ~/.ssh/known_hosts
 *
 * Usage:
 *   node scripts/smoke-public-sftp.mjs
 */
import { SftpClient } from "../index.mjs";

const servers = [
  {
    name: "Rebex (read-only)",
    host: "test.rebex.net",
    port: 22,
    username: "demo",
    password: "password",
    readOnly: true,
    probePath: "/",
  },
  {
    name: "Wing FTP demo",
    host: "demo.wftpserver.com",
    port: 2222,
    username: "demo",
    password: "demo",
    readOnly: false,
    probePath: "/",
  },
];

async function exercise(server) {
  console.log(`\n=== ${server.name} (${server.host}:${server.port}) ===`);
  const client = await SftpClient.connect({
    host: server.host,
    port: server.port,
    username: server.username,
    password: server.password,
  });

  try {
    const names = await client.readDir(server.probePath);
    console.log(`readDir(${server.probePath}): ${names.length} entries`);
    console.log(`  sample: ${names.slice(0, 8).join(", ") || "(empty)"}`);

    const file = names.find((n) => n && !n.startsWith(".") && n.includes("."));
    if (file) {
      const remotePath = server.probePath === "/" ? `/${file}` : `${server.probePath}/${file}`;
      try {
        const buf = await client.readFile(remotePath);
        console.log(`readFile(${remotePath}): ${buf.length} bytes`);
      } catch (error) {
        console.log(`readFile(${remotePath}) skipped: ${error.message}`);
      }
    }

    if (!server.readOnly) {
      const stamp = Date.now();
      const tmpDir = `/upload/sftp-client-native-${stamp}`;
      const tmpFile = `${tmpDir}/probe.txt`;
      try {
        await client.createDir(tmpDir);
        await client.writeFile(tmpFile, Buffer.from("sftp-client-native smoke\n"));
        const roundTrip = await client.readFile(tmpFile);
        console.log(`write/read round-trip: ${roundTrip.toString("utf8").trim()}`);
        await client.removeFile(tmpFile);
        await client.removeDir(tmpDir);
        console.log("cleanup: ok");
      } catch (error) {
        console.log(`write probe skipped/failed: ${error.message}`);
      }
    }
  } finally {
    await client.close();
    console.log("close: ok");
  }
}

let failures = 0;
for (const server of servers) {
  try {
    await exercise(server);
  } catch (error) {
    failures += 1;
    console.error(`FAILED ${server.name}: ${error.message}`);
  }
}

if (failures > 0) {
  console.error(`\n${failures}/${servers.length} server(s) failed`);
  process.exitCode = 1;
} else {
  console.log(`\nAll ${servers.length} public servers passed password-auth smoke checks`);
}
