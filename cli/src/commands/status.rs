use anyhow::Result;
use sidedns_core::{IpcClient, IpcRequest, IpcResponse};

pub async fn run() -> Result<()> {
    let client = IpcClient::default();

    if !client.is_running().await {
        println!("daemon: stopped");
        return Ok(());
    }

    match client.send(IpcRequest::Status).await? {
        IpcResponse::Status {
            running,
            rule_count,
        } => {
            println!("Daemon:  {}", if running { "running" } else { "stopped" });
            println!("Rules:   {rule_count}");
        },
        IpcResponse::Error(e) => eprintln!("error: {e}"),
        _ => {},
    }

    Ok(())
}
