use std::fmt::Display;
use std::sync::Arc;

use napi::bindgen_prelude::Buffer;
use napi::{Error as NapiError, Result};
use napi_derive::napi;
use russh::client::{self, Handler};
use russh::keys::{self, PublicKeyOrCertificate};
use russh_sftp::client::SftpSession;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{RwLock, RwLockReadGuard};

fn to_napi_error(error: impl Display) -> NapiError {
    NapiError::from_reason(error.to_string())
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
    pub password: String,
    pub port: Option<u16>,
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
        let port = options.port.unwrap_or(22);
        let config = Arc::new(client::Config::default());
        let handler = KnownHostsHandler {
            host: options.host.clone(),
            port,
        };

        let mut ssh = client::connect(config, (options.host.as_str(), port), handler).await.map_err(to_napi_error)?;

        let auth = ssh.authenticate_password(&options.username, &options.password).await.map_err(to_napi_error)?;
        if !auth.success() {
            return Err(NapiError::from_reason("SSH password authentication was rejected"));
        }

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
