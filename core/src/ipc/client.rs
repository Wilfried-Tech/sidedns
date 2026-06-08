use interprocess::local_socket::{GenericFilePath, ToFsName, tokio::prelude::*};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

use super::IpcError;
use super::message::{DnsEvent, IpcRequest, IpcResponse};
use anyhow::Result;

/// Client for communicating with a running [`super::IpcServer`].
///
/// Uses Unix domain sockets on Linux/macOS and named pipes on Windows.
/// Each method opens a fresh connection for the duration of the call.
///
/// # Examples
///
/// ```no_run
/// # use sidedns_core::{IpcClient, IpcRequest};
/// # #[tokio::main] async fn main() -> anyhow::Result<()> {
/// let client = IpcClient::default();
///
/// if client.is_running().await {
///     let response = client.send(IpcRequest::List).await?;
///     println!("{response:?}");
/// }
/// # Ok(())
/// # }
/// ```
pub struct IpcClient {
    socket_path: String,
}

impl Default for IpcClient {
    fn default() -> Self {
        Self {
            socket_path: crate::IPC_SOCKET_PATH.to_string(),
        }
    }
}

impl IpcClient {
    /// Create a client targeting a custom socket path.
    ///
    /// Prefer [`IpcClient::default`] in production code.
    /// This method exists primarily for test isolation.
    pub fn with_path(path: impl Into<String>) -> Self {
        Self {
            socket_path: path.into(),
        }
    }

    /// Return `true` if a daemon is reachable on the configured socket.
    ///
    /// This is a lightweight connection probe — no request is sent.
    pub async fn is_running(&self) -> bool {
        let Ok(name) = self.socket_path.as_str().to_fs_name::<GenericFilePath>() else {
            return false;
        };
        LocalSocketStream::connect(name).await.is_ok()
    }

    /// Send a single request to the daemon and await its response.
    ///
    /// Opens a connection, writes the request as a newline-terminated JSON
    /// object, reads one response line, then closes the connection.
    ///
    /// # Errors
    ///
    /// Returns [`IpcError`] if the connection fails, the daemon is not running,
    /// or serialization of the request or response fails.
    pub async fn send(&self, request: IpcRequest) -> Result<IpcResponse> {
        let name = self.socket_path.as_str().to_fs_name::<GenericFilePath>()?;
        let conn = LocalSocketStream::connect(name)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to IPC server. Is it running?\n{e}"))?;

        let (reader, mut writer) = tokio::io::split(conn);

        let mut payload = serde_json::to_string(&request)?;
        payload.push('\n');
        writer.write_all(payload.as_bytes()).await?;
        writer.flush().await?;

        let mut lines = BufReader::new(reader).lines();
        let line = lines.next_line().await?.ok_or(IpcError::ConnectionClosed)?;

        Ok(serde_json::from_str(&line)?)
    }

    /// Subscribe to daemon events and receive them through an async channel.
    ///
    /// Connects to the daemon, sends a [`IpcRequest::Subscribe`] command, and
    /// immediately returns a [`mpsc::Receiver`] that yields [`DnsEvent`] values
    /// as they arrive. The background task exits when the connection is closed.
    ///
    /// Dropping the returned receiver causes the background task to exit on the
    /// next event delivery attempt.
    ///
    /// # Errors
    ///
    /// Returns [`IpcError`] if the initial connection or subscribe handshake fails.
    pub async fn subscribe(&self) -> Result<mpsc::Receiver<DnsEvent>> {
        let name = self.socket_path.as_str().to_fs_name::<GenericFilePath>()?;
        let conn = LocalSocketStream::connect(name)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to IPC server. Is it running?\n{e}"))?;

        let (reader, mut writer) = tokio::io::split(conn);

        let mut payload = serde_json::to_string(&IpcRequest::Subscribe)?;
        payload.push('\n');
        writer.write_all(payload.as_bytes()).await?;
        writer.flush().await?;

        let (tx, rx) = mpsc::channel(32);

        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();

            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(response) = serde_json::from_str::<IpcResponse>(&line) else {
                    continue;
                };

                if let IpcResponse::Event(event) = response
                    && tx.send(event).await.is_err()
                {
                    break;
                }
            }
        });

        Ok(rx)
    }
}
