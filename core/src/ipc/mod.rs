use thiserror::Error;

mod client;
mod handler;
mod message;
mod server;

pub use client::IpcClient;
pub use message::{DnsEvent, IpcRequest, IpcResponse};
pub use server::{IpcHandler, IpcServer};

/// Errors specific to the IPC layer.
///
/// I/O and serialization errors are propagated transparently via [`anyhow`].
/// This type covers only domain-level failure cases that callers may want
/// to handle explicitly.
#[derive(Error, Debug)]
pub enum IpcError {
    /// The remote end closed the connection before sending a complete response.
    #[error("IPC connection was closed unexpectedly")]
    ConnectionClosed,
}
