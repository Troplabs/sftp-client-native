# Security policy

## Reporting a vulnerability

Please do not open a public issue for a suspected security vulnerability. Use GitHub's private vulnerability reporting for this repository when it is enabled. If it is not enabled, contact the maintainers privately through the Troplabs GitHub organization.

Include the affected version, impact, and a minimal reproduction where possible. Do not include real credentials or private keys.

## SSH host verification

The client uses the standard OpenSSH known_hosts file and rejects unknown or changed host keys. Do not weaken this behavior to unconditional acceptance. Verify a server's host key through a trusted channel before recording it.
