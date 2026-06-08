use std::time::Duration;

use anyhow::Result;
use sidedns_core::{IpcClient, IpcRequest};

use crate::{
    DaemonStartArgs,
    daemon::{build_daemon, run_daemon},
};

/// Start the daemon as a detached background process.
pub async fn start(args: DaemonStartArgs) -> Result<()> {
    let daemon = build_daemon();
    let client = IpcClient::default();
    let background = args.background && !args.no_background;

    let ipc_running = client.is_running().await;
    let daemon_running = daemon.is_running();

    if ipc_running {
        println!("Daemon already running");
    } else if daemon_running {
        println!("There are some problem! Restarting...");
        stop().await?;
        run_daemon(background).await?;
    } else {
        run_daemon(background).await?;
    }
    Ok(())
}

/// Stop the running daemon via its PID file.
pub async fn stop() -> Result<()> {
    let daemon = build_daemon();
    let client = IpcClient::default();

    let ipc_running = client.is_running().await;
    let daemon_running = daemon.is_running();

    if !(ipc_running || daemon_running) {
        println!("Daemon not running");
        return Ok(());
    }

    if ipc_running {
        client.send(IpcRequest::Stop).await?;
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if !client.is_running().await {
                break;
            }
        }
    } else if daemon_running {
        if daemon.is_service_installed() {
            daemon
                .stop()
                .map_err(|err| anyhow::anyhow!("Failed to stop daemon\n{err}"))?;
        } else {
            if let Some(pid) = daemon.running_pid() {
                println!("Can't stop daemon Pid: {pid}, please kill it");
                return Ok(());
            }
        }
    }

    println!("Daemon stopped");
    Ok(())
}
