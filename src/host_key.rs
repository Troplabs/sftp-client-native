use std::fmt::Display;
use std::sync::{Arc, OnceLock};

use napi::bindgen_prelude::Either;
use napi::{Error as NapiError, Result};
use russh::client::Handler;
use russh::keys::ssh_key::{Fingerprint, HashAlg, PublicKey};
use russh::keys::{self, PublicKeyOrCertificate};

use crate::ConnectOptions;

pub(crate) enum HostKeyPolicy {
    KnownHosts,
    Pinned(Vec<Fingerprint>),
}

pub(crate) fn resolve_host_key_policy(options: &ConnectOptions) -> Result<HostKeyPolicy> {
    let entries: &[String] = match &options.host_key_fingerprint {
        None => return Ok(HostKeyPolicy::KnownHosts),
        Some(Either::A(single)) => std::slice::from_ref(single),
        Some(Either::B(list)) => list.as_slice(),
    };

    if entries.is_empty() {
        return Err(NapiError::from_reason(
            "hostKeyFingerprint must contain at least one fingerprint",
        ));
    }

    let mut pins = Vec::with_capacity(entries.len());
    for entry in entries {
        let fingerprint = entry.trim().trim_end_matches('=').parse::<Fingerprint>().map_err(|_| {
            NapiError::from_reason(format!(
                "hostKeyFingerprint entry is not a valid OpenSSH SHA256/SHA512 fingerprint: {entry}"
            ))
        })?;
        pins.push(fingerprint);
    }
    Ok(HostKeyPolicy::Pinned(pins))
}

fn pin_matches(key: &PublicKey, pins: &[Fingerprint]) -> bool {
    pins.iter().any(|pin| key.fingerprint(pin.algorithm()) == *pin)
}

#[derive(Debug)]
pub(crate) enum HostKeyHandlerError {
    Ssh(russh::Error),
    Rejected(String),
}

impl From<russh::Error> for HostKeyHandlerError {
    fn from(error: russh::Error) -> Self {
        Self::Ssh(error)
    }
}

impl Display for HostKeyHandlerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ssh(error) => error.fmt(f),
            Self::Rejected(message) => f.write_str(message),
        }
    }
}

pub(crate) struct HostKeyHandler {
    host: String,
    port: u16,
    policy: HostKeyPolicy,
    presented: Arc<OnceLock<String>>,
}

impl HostKeyHandler {
    pub(crate) fn new(host: String, port: u16, policy: HostKeyPolicy, presented: Arc<OnceLock<String>>) -> Self {
        Self {
            host,
            port,
            policy,
            presented,
        }
    }
}

impl Handler for HostKeyHandler {
    type Error = HostKeyHandlerError;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> std::result::Result<bool, Self::Error> {
        let key = server_public_key.public_key();
        let fp = key.fingerprint(HashAlg::Sha256).to_string();
        let _ = self.presented.set(fp.clone());
        let alg = key.algorithm();

        match &self.policy {
            HostKeyPolicy::Pinned(pins) => {
                if pin_matches(&key, pins) {
                    Ok(true)
                } else {
                    Err(HostKeyHandlerError::Rejected(format!(
                        "Host key for {}:{} ({alg}, {fp}) does not match any hostKeyFingerprint",
                        self.host, self.port
                    )))
                }
            }
            HostKeyPolicy::KnownHosts => match keys::check_known_hosts(&self.host, self.port, &key) {
                Ok(true) => Ok(true),
                Ok(false) => Err(HostKeyHandlerError::Rejected(format!(
                    "Host key for {}:{} ({alg}, {fp}) is not in ~/.ssh/known_hosts",
                    self.host, self.port
                ))),
                Err(keys::Error::KeyChanged { line }) => Err(HostKeyHandlerError::Rejected(format!(
                    "Host key for {}:{} ({alg}, {fp}) does not match ~/.ssh/known_hosts line {line}",
                    self.host, self.port
                ))),
                Err(e) => Err(HostKeyHandlerError::Rejected(format!(
                    "Cannot verify host key for {}:{} against ~/.ssh/known_hosts: {e}",
                    self.host, self.port
                ))),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PUBLIC_KEY: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ";
    const TEST_SHA256: &str = "SHA256:T7SvZ2cslqpPj6nKzitCBHHlpVF3r3MvLwmFL0fk0IE";
    const TEST_SHA512: &str =
        "SHA512:nVQqsotKGzpZQA4UfOc+vusVYbH6ucWGJTv7Se/sa9VCgHa2yZLjooBLYogvB1EVN1OWqjqn3TVL6IrfL/iCkQ";

    fn options_with_pin(pin: Option<Either<String, Vec<String>>>) -> ConnectOptions {
        ConnectOptions {
            host: "example.com".into(),
            username: "user".into(),
            port: None,
            password: Some("secret".into()),
            private_key: None,
            private_key_path: None,
            passphrase: None,
            certificate: None,
            certificate_path: None,
            agent: None,
            agent_socket: None,
            host_key_fingerprint: pin,
        }
    }

    fn resolve_pins(pin: Option<Either<String, Vec<String>>>) -> Result<Vec<Fingerprint>> {
        match resolve_host_key_policy(&options_with_pin(pin))? {
            HostKeyPolicy::KnownHosts => panic!("expected pinned policy"),
            HostKeyPolicy::Pinned(pins) => Ok(pins),
        }
    }

    #[test]
    fn no_pin_uses_known_hosts() {
        assert!(matches!(
            resolve_host_key_policy(&options_with_pin(None)).unwrap(),
            HostKeyPolicy::KnownHosts
        ));
    }

    #[test]
    fn parses_sha256_and_sha512() {
        let pins = resolve_pins(Some(Either::B(vec![TEST_SHA256.to_string(), TEST_SHA512.to_string()]))).unwrap();
        assert_eq!(pins.len(), 2);
        assert!(matches!(pins[0].algorithm(), HashAlg::Sha256));
        assert!(matches!(pins[1].algorithm(), HashAlg::Sha512));
    }

    #[test]
    fn accepts_padded_and_whitespace_wrapped_entries() {
        let pins = resolve_pins(Some(Either::A(format!("  {TEST_SHA256}=  ")))).unwrap();
        assert_eq!(pins[0].to_string(), TEST_SHA256);
    }

    #[test]
    fn rejects_invalid_entries() {
        for bad in ["MD5:aa:bb", "SHA256:not!!base64", "garbage", "  "] {
            let error = resolve_pins(Some(Either::A(bad.into()))).unwrap_err();
            assert!(
                error.to_string().contains("is not a valid OpenSSH SHA256/SHA512 fingerprint"),
                "{bad}: {error}"
            );
        }
    }

    #[test]
    fn rejects_empty_array() {
        let error = resolve_pins(Some(Either::B(vec![]))).unwrap_err();
        assert!(
            error.to_string().contains("hostKeyFingerprint must contain at least one fingerprint"),
            "{error}"
        );
    }

    #[test]
    fn pin_matches_and_mismatches() {
        let key = PublicKey::from_openssh(TEST_PUBLIC_KEY).unwrap();

        let sha256: Fingerprint = TEST_SHA256.parse().unwrap();
        assert!(pin_matches(&key, &[sha256]));

        let sha512: Fingerprint = TEST_SHA512.parse().unwrap();
        assert!(pin_matches(&key, &[sha512]));

        let wrong: Fingerprint = "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".parse().unwrap();
        assert!(!pin_matches(&key, &[wrong]));
    }
}
