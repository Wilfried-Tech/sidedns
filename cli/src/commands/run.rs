use std::net::{Ipv4Addr, SocketAddr};

use crate::{
    cli::RunArgs,
    run::{detect_network_processes, wait_for_port},
};
use anyhow::Result;
use sidedns_core::{DnsEvent, IpcClient, IpcRequest, IpcResponse, certs, logging};
use tokio::process::Command;

/// Run a command with an ephemeral DNS rule for the duration of its execution.
pub async fn run(args: RunArgs) -> Result<()> {
    let client = IpcClient::default();
    logging::init_stdout();

    let RunArgs {
        domain,
        ip,
        port,
        https,
        command,
        detect_timeout,
    } = args;

    let mut ip = ip.unwrap_or(Ipv4Addr::LOCALHOST.into());

    if command.is_empty() {
        tracing::warn!("No command provided, exiting immediately");
        std::process::exit(1);
    }

    let mut child = Command::new(&command[0])
        .args(&command[1..])
        .spawn()
        .map_err(|e| {
            tracing::error!("Failed to launch '{}': {e}.Is it installed?", command[0]);
            anyhow::anyhow!("")
        })?;

    let resolved_port = if let Some(p) = port {
        tracing::info!(
            "Waiting up to {detect_timeout}s for {}:{} to open...",
            ip,
            p
        );
        if !wait_for_port(ip, p, detect_timeout).await {
            tracing::error!("{ip}:{p} did not open within {detect_timeout} seconds");
            std::process::exit(1)
        }
        Ok::<_, anyhow::Error>(p)
    } else {
        tracing::info!("Detecting address (waiting {detect_timeout}s)...");
        if child.id().is_none() {
            tracing::error!("Failed to get PID of the launched command");
            std::process::exit(1)
        }
        let pid = child.id().unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(detect_timeout.into())).await;
        let processes = detect_network_processes(pid);
        match processes.len() {
            0 => {
                tracing::error!(
                    "Process {} did not open any ports within {detect_timeout} seconds, \
                    Try running the command separately to debug, or specify the port manually with --port",
                    command[0]
                );
                std::process::exit(1)
            },
            1 => {
                tracing::info!("Process {} detected on {}", command[0], processes[0].1);
                ip = processes[0].1.ip();
                Ok(processes[0].1.port())
            },
            _ => {
                let selection = inquire::Select::new(
                    "Multiple network listeners detected, select the ones to proxy: ",
                    processes.iter().map(|(_, addr)| addr).collect::<Vec<_>>(),
                )
                .prompt()?;
                ip = selection.ip();
                Ok(selection.port())
            },
        }
    }?;

    let request = IpcRequest::AddEphemeral {
        domain: domain.clone(),
        target: SocketAddr::new(ip, resolved_port),
        https,
    };
    match client.send(request).await? {
        IpcResponse::Ok => {
            let scheme = if https { "https" } else { "http" };
            tracing::info!(
                "{domain} → http://{ip}:{resolved_port} (proxied at {scheme}://{domain})"
            );
            if https && !certs::Ca::is_installed() {
                tracing::warn!(
                    "TLS is enabled for this rule, but the root CA certificate is not installed. \
                    Please install and trust it to trust the proxy's certificate."
                );
            }
            tracing::info!("Launching: {}", command.join(" "));
        },
        IpcResponse::Error(e) => {
            tracing::error!("Failed to add ephemeral rule: {e}");
            std::process::exit(1)
        },
        _ => {},
    }

    tracing::info!("Press Ctrl-C to stop");

    let mut daemon_event = client.subscribe().await?;

    let stop_event = async move {
        while let Some(event) = daemon_event.recv().await {
            if matches!(event, DnsEvent::DaemonStopped) {
                return;
            }
        }
    };

    let exit_code = tokio::select! {
        biased;
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Stopping {} (Ctrl-C Received)...", command[0]);
            child.kill().await?;
            0
        }
        _ = stop_event => {
            tracing::info!("Daemon Stopped, Killing process...");
            child.kill().await?;
            std::process::exit(0);
        }
        status = child.wait() => {
            match status {
                Ok(s)  => s.code().unwrap_or(1),
                Err(e) => { tracing::error!("Process error: {e}"); 1 }
            }
        }

    };
    let request = IpcRequest::RemoveEphemeral {
        domain: domain.clone(),
    };
    match client.send(request).await {
        Ok(IpcResponse::Ok) => tracing::info!("Ephemeral rule for '{domain}' removed"),
        Ok(IpcResponse::Error(e)) => tracing::warn!("failed to remove ephemeral rule: {e}"),
        Err(e) => tracing::warn!("IPC error during cleanup: {e}"),
        _ => {},
    }

    if exit_code != 0 {
        std::process::exit(exit_code);
    }

    Ok(())
}
