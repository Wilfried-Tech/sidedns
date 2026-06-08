use anyhow::Result;
use sidedns_core::{DnsEvent, IpcClient, logging};

pub async fn run() -> Result<()> {
    let client = IpcClient::default();
    logging::init_stdout();

    let mut events = client.subscribe().await?;
    tracing::info!("Event Listening Started");
    loop {
        tokio::select! {
            biased;
             _ = tokio::signal::ctrl_c() => {
                tracing::info!("Ctrl-C Received, Stopping...");
                break
            },
            event = events.recv() => {
                if let Some(event) = event {
                    match event {
                        DnsEvent::RuleAdded(rule) => {
                            tracing::info!("Rule Added: {rule}");
                        },
                        DnsEvent::RuleRemoved(rule)=> {
                            tracing::info!("Rule Removed: {rule}");
                        },
                        DnsEvent::EphemeralAdded(rule) => {
                            tracing::info!("Ephemeral Rule Added: {rule}");
                        },
                        DnsEvent::EphemeralRemoved(rule)=> {
                            tracing::info!("Ephemeral Rule Removed: {rule}");
                        },
                        DnsEvent::DaemonStopped => {
                            tracing::info!("Daemon Stopped");
                        }
                    }
                } else {
                    tracing::info!("Connection Closed, Stopping...");
                    break;
                }
            }
        }
    }
    tracing::info!("Event Listening Stopped");
    Ok(())
}
