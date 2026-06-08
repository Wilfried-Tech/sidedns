use std::sync::Arc;

use async_trait::async_trait;
use interprocess::local_socket::{GenericFilePath, ListenerOptions, ToFsName, tokio::prelude::*};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use super::message::{DnsEvent, IpcRequest, IpcResponse};
use anyhow::Result;

/// Implemented by the daemon's shared state to handle incoming IPC requests.
///
/// The implementation receives decoded requests and returns encoded responses.
/// It also provides a broadcast receiver for the event stream used by
/// [`IpcServer`] to forward events to subscribed clients.
#[async_trait]
pub trait IpcHandler: Send + Sync + 'static {
    /// Process a single request and return a response.
    async fn handle(&self, request: IpcRequest) -> IpcResponse;

    /// Subscribe to the internal event broadcast channel.
    ///
    /// Each call returns an independent receiver. The server creates one per
    /// subscriber connection.
    fn subscribe_events(&self) -> broadcast::Receiver<DnsEvent>;
}

/// Listens on a local socket and dispatches connections to an [`IpcHandler`].
///
/// Uses Unix domain sockets on Linux/macOS and named pipes on Windows.
/// The server shuts down cleanly when the provided [`CancellationToken`] is cancelled.
pub struct IpcServer {
    socket_path: String,
}

impl Default for IpcServer {
    fn default() -> Self {
        Self {
            socket_path: crate::IPC_SOCKET_PATH.to_string(),
        }
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        #[cfg(not(windows))]
        if std::path::Path::new(&self.socket_path).exists() {
            if let Err(e) = std::fs::remove_file(&self.socket_path) {
                tracing::error!("Failed to remove IpcServer socket file: {e}");
            } else {
                tracing::info!("IpcServer socket file removed");
            }
        }
    }
}

impl IpcServer {
    /// Create a server bound to a custom socket path.
    ///
    /// Prefer [`IpcServer::default`] in production code.
    /// This method exists primarily for test isolation.
    pub fn with_path(path: impl Into<String>) -> Self {
        Self {
            socket_path: path.into(),
        }
    }

    /// Start accepting connections until `token` is cancelled.
    ///
    /// Removes a stale socket file at startup on Unix. On shutdown,
    /// the socket file is removed again so subsequent starts are clean.
    ///
    /// # Errors
    ///
    /// Returns [`IpcError`] if the socket cannot be bound or if an
    /// unrecoverable I/O error occurs during the accept loop.
    #[tracing::instrument(skip(self, handler, token), name = "IPC Server")]
    pub async fn serve<H: IpcHandler>(
        &self,
        handler: Arc<H>,
        token: CancellationToken,
    ) -> Result<()> {
        #[cfg(not(windows))]
        {
            if let Err(e) = std::fs::remove_file(&self.socket_path)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                return Err(e.into());
            }
        }

        let name = self.socket_path.as_str().to_fs_name::<GenericFilePath>()?;
        let options = ListenerOptions::new().name(name);

        #[cfg(target_os = "linux")]
        let options = {
            use interprocess::os::unix::local_socket::ListenerOptionsExt;
            options.mode(0o666)
        };

        #[cfg(target_os = "macos")]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&self.socket_path) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o666);
                let _ = std::fs::set_permissions(&self.socket_path, perms);
            }
        }

        #[cfg(windows)]
        let options = {
            // On Windows, set a security descriptor that allows all users to connect to the named pipe.
            // This is necessary for the CLI to work when run from an unelevated prompt.
            // D:(A;;GA;;;BA)(A;;GA;;;SY)(A;;GA;;;AU)

            use interprocess::os::windows::{
                local_socket::ListenerOptionsExt, security_descriptor::SecurityDescriptor,
            };
            use widestring::u16cstr;
            let sd =
                SecurityDescriptor::deserialize(u16cstr!("D:(A;;GA;;;BA)(A;;GA;;;SY)(A;;GA;;;AU)"))
                    .map_err(std::io::Error::other)?;
            options.security_descriptor(sd)
        };

        let listener = options.create_tokio()?;

        tracing::info!("Started");

        let result = self.accept_loop(&listener, handler, token).await;

        tracing::info!("Stopped");

        result
    }

    async fn accept_loop<H: IpcHandler>(
        &self,
        listener: &LocalSocketListener,
        handler: Arc<H>,
        token: CancellationToken,
    ) -> Result<()> {
        loop {
            tokio::select! {
                biased;
                _ = token.cancelled() => {
                    tracing::info!("Shutdown requested, stopping...");
                    break;
                },
                result = listener.accept() => {
                    let conn = result?;
                    let handler = handler.clone();
                    let token = token.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(conn, handler, token).await {
                            tracing::error!("Connection error: {e}");
                        }
                    });
                }
            }
        }

        Ok(())
    }
}

#[tracing::instrument(skip(conn, handler, token), name = "IPC Server")]
async fn handle_connection<H: IpcHandler>(
    conn: LocalSocketStream,
    handler: Arc<H>,
    token: CancellationToken,
) -> Result<()> {
    let (reader, mut writer) = tokio::io::split(conn);
    let mut lines = BufReader::new(reader).lines();

    let Some(line) = lines.next_line().await? else {
        return Ok(());
    };

    let request: IpcRequest = serde_json::from_str(&line)?;

    if matches!(request, IpcRequest::Subscribe) {
        handle_subscribe(&mut writer, handler, token).await
    } else {
        let response = handler.handle(request).await;
        write_response(&mut writer, &response).await
    }
}

async fn handle_subscribe<W>(
    writer: &mut W,
    handler: Arc<impl IpcHandler>,
    token: CancellationToken,
) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let mut rx = handler.subscribe_events();

    loop {
        tokio::select! {
            biased;
            _ = token.cancelled() => break,
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        write_response(writer, &IpcResponse::Event(event)).await?;
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    Ok(())
}

async fn write_response<W>(writer: &mut W, response: &IpcResponse) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let mut payload = serde_json::to_string(response)?;
    payload.push('\n');
    writer.write_all(payload.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}
