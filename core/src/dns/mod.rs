mod resolver;
mod server;
mod sys;
use std::sync::Arc;

use hickory_server::ServerFuture;
pub use resolver::DomainResolver;
pub use sys::{platform_dns_manager, run_system_dns_manager};
use tokio::net::TcpListener;
use tokio::net::UdpSocket;
use tokio_util::sync::CancellationToken;

use crate::DNS_LISTEN_ADDR;
use server::{DnsHandler, build_upstream_resolver};

/// Start the DNS server on UDP and TCP
///
/// Queries for domains present in [`SharedState`] are answered locally.
/// All other queries are forwarded to the system upstream resolver.
/// Shuts down cleanly when `token` is cancelled.
#[tracing::instrument(skip(resolver, token), name = "DNS Server")]
pub async fn run_dns_server(
    resolver: Arc<DomainResolver>,
    token: CancellationToken,
) -> anyhow::Result<()> {
    let upstream = build_upstream_resolver()?;
    tracing::info!("Upstream DNS configured from system");

    let handler = DnsHandler::new(resolver, upstream);

    let udp = UdpSocket::bind(DNS_LISTEN_ADDR).await?;
    let tcp = TcpListener::bind(DNS_LISTEN_ADDR).await?;

    tracing::info!("Started");
    tracing::info!("Listening on {DNS_LISTEN_ADDR}");

    let mut server = ServerFuture::new(handler);
    server.register_socket(udp);
    server.register_listener(tcp, std::time::Duration::from_secs(5));

    tokio::select! {
        biased;
        _ = token.cancelled() => {
            tracing::info!("Shutdown requested, Stopping...");
            server.shutdown_gracefully().await?;
        }
        result = server.block_until_done() => {
            if let Err(e) = result {
                tracing::error!("Error: {e}");
            }
        }
    }

    tracing::info!("Stopped");
    Ok(())
}
