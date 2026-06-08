#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod systemd;
#[cfg(target_os = "windows")]
mod windows;

mod noop;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

pub use crate::DNS_LISTEN_ADDR;
use crate::SharedState;

/// Abstraction over OS-level DNS configuration.
///
/// All methods must be idempotent — calling `apply` twice with the same
/// domains, or `revert` when nothing is configured, must not error.
pub trait SystemDnsManager: Send + Sync {
    /// Route `domains` through SideDNS.
    fn apply(&self, domains: &[String]) -> anyhow::Result<()>;

    /// Remove all SideDNS routing rules from the system.
    fn revert(&self) -> anyhow::Result<()>;
}

/// Return the best available [`DnsConfigurator`] for the current platform.
///
/// Falls back to [`noop::NoOpConfigurator`] if no suitable implementation
/// is detected (e.g. non-systemd Linux).
pub fn platform_dns_manager() -> Box<dyn SystemDnsManager> {
    #[cfg(target_os = "linux")]
    {
        if systemd::SystemdConfigurator::is_available() {
            return Box::new(systemd::SystemdConfigurator);
        }
        tracing::warn!("systemd-resolved not available — DNS system config disabled");
        return Box::new(noop::NoOpConfigurator);
    }

    #[cfg(target_os = "macos")]
    {
        return Box::new(macos::MacOsConfigurator);
    }

    #[cfg(windows)]
    {
        return Box::new(windows::WindowsNrptConfigurator);
    }

    #[allow(unreachable_code)]
    {
        tracing::warn!(
            "No DNS configurator available for this platform — DNS system config disabled"
        );
        Box::new(noop::NoOpConfigurator)
    }
}

/// Run system level dns config
///
/// Always revert first — cleans up any orphaned config from a previous crash.
#[tracing::instrument(skip(state, token), name = "System DNS Manager")]
pub async fn run_system_dns_manager(
    state: SharedState,
    token: CancellationToken,
) -> anyhow::Result<()> {
    let dns_manager = platform_dns_manager();
    if let Err(e) = dns_manager.revert() {
        tracing::warn!("Pre-start DNS config cleanup failed: {e}");
    }
    let mut events = state.events.subscribe();
    tracing::info!("Started");
    tracing::info!("Applying DNS system configuration...");
    dns_manager.apply(&state.snapshot_all_domains())?;
    loop {
        tokio::select! {
            biased;
            _ = token.cancelled() => {
                tracing::info!("Shutdown requested, Stopping...");
                break;
            }
            res = events.recv() => {
                match res {
                    Ok(event) => match event {
                        crate::ipc::DnsEvent::RuleAdded(_)
                        | crate::ipc::DnsEvent::RuleRemoved(_)
                        | crate::ipc::DnsEvent::EphemeralAdded(_)
                        | crate::ipc::DnsEvent::EphemeralRemoved(_) => {
                            dns_manager.apply(&state.snapshot_all_domains())?;
                        },
                        _ => {}
                    },
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                };
            }
        }
    }
    if let Err(e) = dns_manager.revert() {
        tracing::warn!("Post-shutdown DNS config cleanup failed: {e}");
    } else {
        tracing::info!("DNS config cleanned");
    }
    tracing::info!("Stopped");
    Ok(())
}
