use std::sync::Arc;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::certs;
use crate::ipc::IpcServer;
use crate::state::AppState;
use crate::{DnsEvent, dns, proxy};

/// Run the daemon until a shutdown signal is received.
#[tracing::instrument(skip(token), name = "Daemon")]
pub async fn run(token: Option<CancellationToken>) -> anyhow::Result<()> {
    tracing::info!("Starting");

    let ca = certs::ca::load()
        .map_err(|e| {
            tracing::error!("Failed to load CA certificate: {e}");
            tracing::warn!("HTTPS proxy disabled. Run 'sidedns cert install'.");
            e
        })
        .ok();

    let token = token.unwrap_or_default();
    let state = Arc::new(AppState::new(token.clone()));
    let resolver = Arc::new(dns::DomainResolver::new(state.clone(), ca));

    let rules = state.load_rules();
    if resolver.is_cert_installed() {
        for rule in rules.iter().filter(|r| r.https && !r.is_wildcard()) {
            if resolver.sign_domain(&rule.domain).is_none() {
                tracing::warn!("failed to restore cert for {}", rule.domain);
            }
        }
    }

    let ipc_task = {
        let state = state.clone();
        let token = token.clone();
        tokio::spawn(async move {
            if let Err(e) = IpcServer::default().serve(state, token).await {
                tracing::error!("Ipc Server Run Error: {e}")
            }
        })
    };

    let dns_task = {
        let token = token.clone();
        let resolver = resolver.clone();
        tokio::spawn(async move {
            if let Err(e) = dns::run_dns_server(resolver, token).await {
                tracing::error!("DNS Server Run Error: {e}")
            }
        })
    };

    let proxy_task = {
        let token = token.clone();
        let resolver = resolver.clone();
        tokio::spawn(async move {
            if let Err(e) = proxy::run_proxy_server(resolver, token).await {
                tracing::error!("Proxy Server Run Error: {e}")
            }
        })
    };

    let dns_manager_task = {
        let state = state.clone();
        let token = token.clone();
        tokio::spawn(async move {
            if let Err(e) = dns::run_system_dns_manager(state, token).await {
                tracing::error!("System DNS manager task error: {e}");
            }
        })
    };

    let rules_watch_task = {
        let mut events = state.events.subscribe();
        let state = state.clone();
        let token = token.clone();
        let resolver = resolver.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = token.cancelled() => {
                        break;
                    }
                    event = events.recv() => {
                        match event {
                            Ok(event) => match event {
                                crate::ipc::DnsEvent::RuleAdded(rule)
                                | crate::ipc::DnsEvent::RuleRemoved(rule) => {
                                    resolver.invalidate(&rule);
                                    state.save();
                                }
                                crate::ipc::DnsEvent::EphemeralAdded(rule)
                                | crate::ipc::DnsEvent::EphemeralRemoved(rule) => {
                                    resolver.invalidate(&rule);
                                }
                                _ => {}
                            },
                            Err(broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                };
            }
        })
    };

    #[cfg(unix)]
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    #[cfg(not(unix))]
    let sigterm_recv = std::future::pending::<()>();
    #[cfg(unix)]
    let sigterm_recv = sigterm.recv();

    tokio::select! {
        biased;
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received SIGINT, shutting down");
            token.cancel();
        }
        _ = token.cancelled() => {
            tracing::info!("Shutdown requested");
            state.dispatch(DnsEvent::DaemonStopped);
        }
        _ = sigterm_recv => {
            tracing::info!("Received SIGTERM, shutting down");
            token.cancel();
        }
    }

    let (ipc_res, dns_res, proxy_res, dns_manager_res, rules_watch_res) = tokio::join!(
        ipc_task,
        dns_task,
        proxy_task,
        dns_manager_task,
        rules_watch_task
    );
    if let Err(e) = ipc_res {
        tracing::error!("IPC task error: {e}");
    }
    if let Err(e) = dns_res {
        tracing::error!("DNS task error: {e}");
    }
    if let Err(e) = proxy_res {
        tracing::error!("Proxy task error: {e}");
    }
    if let Err(e) = dns_manager_res {
        tracing::error!("System DNS manager task error: {e}");
    }
    if let Err(e) = rules_watch_res {
        tracing::error!("Rules watch task error: {e}");
    }

    tracing::info!("Daemon stopped");
    Ok(())
}
