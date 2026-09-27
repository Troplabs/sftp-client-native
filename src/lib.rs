use std::fmt::Display;
use std::sync::Arc;

use napi::bindgen_prelude::Buffer;
use napi::{Error as NapiError, Result};
use napi_derive::napi;
use russh::client::{self, Handler};
use russh::keys::agent::AgentIdentity;
use russh::keys::agent::client::AgentClient;
use russh::keys::{self, Certificate, PrivateKeyWithHashAlg, PublicKeyOrCertificate, decode_secret_key};
use russh_sftp::client::SftpSession;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{RwLock, RwLockReadGuard};

fn to_napi_error(error: impl Display) -> NapiError {
    NapiError::from_reason(error.to_string())
}

fn load_private_key(
    key_material: Option<&str>,
    key_path: Option<&str>,
    passphrase: Option<&str>,
) -> Result<russh::keys::PrivateKey> {
    if let Some(path) = key_path {
        keys::load_secret_key(path, passphrase).map_err(to_napi_error)
    } else if let Some(material) = key_material {
        decode_secret_key(material, passphrase).map_err(to_napi_error)
    } else {
        Err(NapiError::from_reason(
            "privateKey or privateKeyPath is required for public key authentication",
        ))
    }
}

fn load_certificate(cert_material: Option<&str>, cert_path: Option<&str>) -> Result<Certificate> {
    if let Some(path) = cert_path {
        keys::load_openssh_certificate(path).map_err(to_napi_error)
    } else if let Some(material) = cert_material {
        Certificate::from_openssh(material).map_err(to_napi_error)
    } else {
        Err(NapiError::from_reason(
            "certificate or certificatePath is required for certificate authentication",
        ))
    }
}

struct KnownHostsHandler {
    host: String,
    port: u16,
}

impl Handler for KnownHostsHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> std::result::Result<bool, Self::Error> {
        Ok(keys::check_known_hosts(&self.host, self.port, &server_public_key.public_key()).unwrap_or(false))
    }
}

#[napi(object)]
pub struct ConnectOptions {
    pub host: String,
    pub username: String,
    pub port: Option<u16>,
    pub password: Option<String>,
    pub private_key: Option<String>,
    pub private_key_path: Option<String>,
    pub passphrase: Option<String>,
    pub certificate: Option<String>,
    pub certificate_path: Option<String>,
    pub agent: Option<bool>,
    pub agent_socket: Option<String>,
}

enum AuthMode {
    Password(String),
    PrivateKey {
        key_material: Option<String>,
        key_path: Option<String>,
        passphrase: Option<String>,
        cert_material: Option<String>,
        cert_path: Option<String>,
    },
    Agent {
        socket: Option<String>,
    },
}

fn resolve_auth_mode(options: &ConnectOptions) -> Result<AuthMode> {
    let password_mode = options.password.is_some();
    let key_mode = options.private_key.is_some() || options.private_key_path.is_some();
    let agent_mode = options.agent == Some(true) || options.agent_socket.is_some();
    let cert_provided = options.certificate.is_some() || options.certificate_path.is_some();

    if options.private_key.is_some() && options.private_key_path.is_some() {
        return Err(NapiError::from_reason(
            "privateKey and privateKeyPath are mutually exclusive",
        ));
    }

    if options.certificate.is_some() && options.certificate_path.is_some() {
        return Err(NapiError::from_reason(
            "certificate and certificatePath are mutually exclusive",
        ));
    }

    if options.passphrase.is_some() && !key_mode {
        return Err(NapiError::from_reason(
            "passphrase is only valid with privateKey or privateKeyPath",
        ));
    }

    if cert_provided && (password_mode || agent_mode) {
        return Err(NapiError::from_reason(
            "certificate/certificatePath can only be used with privateKey or privateKeyPath",
        ));
    }

    if cert_provided && !key_mode {
        return Err(NapiError::from_reason(
            "certificate/certificatePath requires privateKey or privateKeyPath",
        ));
    }

    let mode_count = usize::from(password_mode) + usize::from(key_mode) + usize::from(agent_mode);
    if mode_count != 1 {
        return Err(NapiError::from_reason(
            "ConnectOptions requires exactly one auth mode: password, privateKey/privateKeyPath, or agent/agentSocket",
        ));
    }

    if password_mode {
        return Ok(AuthMode::Password(
            options.password.clone().expect("password_mode checked"),
        ));
    }

    if key_mode {
        return Ok(AuthMode::PrivateKey {
            key_material: options.private_key.clone(),
            key_path: options.private_key_path.clone(),
            passphrase: options.passphrase.clone(),
            cert_material: options.certificate.clone(),
            cert_path: options.certificate_path.clone(),
        });
    }

    Ok(AuthMode::Agent {
        socket: options.agent_socket.clone(),
    })
}

async fn authenticate_session(
    ssh: &mut client::Handle<KnownHostsHandler>,
    username: &str,
    mode: AuthMode,
) -> Result<()> {
    match mode {
        AuthMode::Password(password) => {
            let auth = ssh.authenticate_password(username, password).await.map_err(to_napi_error)?;
            if !auth.success() {
                return Err(NapiError::from_reason("SSH password authentication was rejected"));
            }
            Ok(())
        }
        AuthMode::PrivateKey {
            key_material,
            key_path,
            passphrase,
            cert_material,
            cert_path,
        } => {
            let key = load_private_key(key_material.as_deref(), key_path.as_deref(), passphrase.as_deref())?;
            let key = Arc::new(key);

            let auth = if cert_material.is_some() || cert_path.is_some() {
                let cert = load_certificate(cert_material.as_deref(), cert_path.as_deref())?;
                ssh.authenticate_openssh_cert(username, key, cert).await.map_err(to_napi_error)?
            } else {
                let hash_alg = ssh.best_supported_rsa_hash().await.map_err(to_napi_error)?.flatten();
                ssh.authenticate_publickey(username, PrivateKeyWithHashAlg::new(key, hash_alg))
                    .await
                    .map_err(to_napi_error)?
            };

            if !auth.success() {
                let message = if cert_material.is_some() || cert_path.is_some() {
                    "SSH certificate authentication was rejected"
                } else {
                    "SSH public key authentication was rejected"
                };
                return Err(NapiError::from_reason(message));
            }
            Ok(())
        }
        AuthMode::Agent { socket } => authenticate_with_agent(ssh, username, socket.as_deref()).await,
    }
}

async fn authenticate_with_agent(
    ssh: &mut client::Handle<KnownHostsHandler>,
    username: &str,
    socket: Option<&str>,
) -> Result<()> {
    #[cfg(unix)]
    {
        let mut agent = match socket {
            Some(path) => AgentClient::connect_uds(path).await.map_err(to_napi_error)?,
            None => AgentClient::connect_env().await.map_err(to_napi_error)?,
        };
        try_agent_identities(ssh, username, &mut agent).await
    }

    #[cfg(windows)]
    {
        match socket {
            Some(path) => {
                let mut agent = AgentClient::connect_named_pipe(path).await.map_err(to_napi_error)?;
                try_agent_identities(ssh, username, &mut agent).await
            }
            None => {
                let mut agent = AgentClient::connect_pageant().await.map_err(to_napi_error)?;
                try_agent_identities(ssh, username, &mut agent).await
            }
        }
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = (ssh, username, socket);
        Err(NapiError::from_reason(
            "SSH agent authentication is not supported on this platform",
        ))
    }
}

async fn try_agent_identities<S>(
    ssh: &mut client::Handle<KnownHostsHandler>,
    username: &str,
    agent: &mut AgentClient<S>,
) -> Result<()>
where
    S: russh::keys::agent::client::AgentStream + Unpin + Send,
{
    let identities = agent.request_identities().await.map_err(to_napi_error)?;
    let hash_alg = ssh.best_supported_rsa_hash().await.map_err(to_napi_error)?.flatten();

    let mut saw_identity = false;
    for identity in identities {
        saw_identity = true;
        let auth = match identity {
            AgentIdentity::PublicKey { key, .. } => {
                ssh.authenticate_publickey_with(username, key, hash_alg, agent).await.map_err(to_napi_error)?
            }
            AgentIdentity::Certificate { certificate, .. } => ssh
                .authenticate_certificate_with(username, certificate, hash_alg, agent)
                .await
                .map_err(to_napi_error)?,
        };
        if auth.success() {
            return Ok(());
        }
    }

    if !saw_identity {
        return Err(NapiError::from_reason("SSH agent has no identities available"));
    }

    Err(NapiError::from_reason("SSH agent authentication was rejected"))
}

#[napi(js_name = "SftpClient")]
pub struct JsSftpClient {
    sftp: SftpSession,
    ssh: client::Handle<KnownHostsHandler>,
    closed: RwLock<bool>,
}

impl JsSftpClient {
    async fn ensure_open(&self) -> Result<RwLockReadGuard<'_, bool>> {
        let guard = self.closed.read().await;
        if *guard {
            return Err(NapiError::from_reason("SFTP client is closed"));
        }
        Ok(guard)
    }
}

#[napi]
impl JsSftpClient {
    #[napi(factory)]
    pub async fn connect(options: ConnectOptions) -> Result<Self> {
        let auth_mode = resolve_auth_mode(&options)?;
        let port = options.port.unwrap_or(22);
        let config = Arc::new(client::Config::default());
        let handler = KnownHostsHandler {
            host: options.host.clone(),
            port,
        };

        let mut ssh = client::connect(config, (options.host.as_str(), port), handler).await.map_err(to_napi_error)?;

        authenticate_session(&mut ssh, &options.username, auth_mode).await?;

        let channel = ssh.channel_open_session().await.map_err(to_napi_error)?;
        channel.request_subsystem(true, "sftp").await.map_err(to_napi_error)?;
        let sftp = SftpSession::new(channel.into_stream()).await.map_err(to_napi_error)?;

        Ok(Self {
            sftp,
            ssh,
            closed: RwLock::new(false),
        })
    }

    #[napi]
    pub async fn read_dir(&self, remote_path: String) -> Result<Vec<String>> {
        let _open = self.ensure_open().await?;
        let entries = self.sftp.read_dir(remote_path).await.map_err(to_napi_error)?;
        Ok(entries.map(|entry| entry.file_name()).collect())
    }

    #[napi]
    pub async fn read_file(&self, remote_path: String) -> Result<Buffer> {
        let _open = self.ensure_open().await?;
        let mut file = self.sftp.open(remote_path).await.map_err(to_napi_error)?;
        let mut content = Vec::new();
        file.read_to_end(&mut content).await.map_err(to_napi_error)?;
        file.close().await.map_err(to_napi_error)?;
        Ok(content.into())
    }

    #[napi]
    pub async fn write_file(&self, remote_path: String, content: Buffer) -> Result<()> {
        let _open = self.ensure_open().await?;
        let mut file = self.sftp.create(remote_path).await.map_err(to_napi_error)?;
        file.write_all(content.as_ref()).await.map_err(to_napi_error)?;
        file.sync_all().await.map_err(to_napi_error)?;
        file.close().await.map_err(to_napi_error)
    }

    #[napi]
    pub async fn create_dir(&self, remote_path: String) -> Result<()> {
        let _open = self.ensure_open().await?;
        self.sftp.create_dir(remote_path).await.map_err(to_napi_error)
    }

    #[napi]
    pub async fn remove_file(&self, remote_path: String) -> Result<()> {
        let _open = self.ensure_open().await?;
        self.sftp.remove_file(remote_path).await.map_err(to_napi_error)
    }

    #[napi]
    pub async fn remove_dir(&self, remote_path: String) -> Result<()> {
        let _open = self.ensure_open().await?;
        self.sftp.remove_dir(remote_path).await.map_err(to_napi_error)
    }

    #[napi]
    pub async fn rename(&self, old_remote_path: String, new_remote_path: String) -> Result<()> {
        let _open = self.ensure_open().await?;
        self.sftp.rename(old_remote_path, new_remote_path).await.map_err(to_napi_error)
    }

    #[napi]
    pub async fn close(&self) -> Result<()> {
        let mut closed = self.closed.write().await;
        if *closed {
            return Ok(());
        }
        *closed = true;

        let sftp_close = self.sftp.close().await.map_err(to_napi_error);
        let ssh_disconnect = self
            .ssh
            .disconnect(russh::Disconnect::ByApplication, "client closed", "")
            .await
            .map_err(to_napi_error);

        sftp_close?;
        ssh_disconnect
    }
}
