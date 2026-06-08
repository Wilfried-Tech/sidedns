use anyhow::{Result, bail};
use clap::Parser;
use daemon_kit::{Daemon, DaemonConfig};
use sidedns_core::{APP_DATA_DIR, APP_NAME, DAEMON_ENV, IpcClient, logging};

use crate::cli::{Cli, Command, DaemonAction};

pub fn build_daemon() -> Daemon {
    Daemon::new(
        DaemonConfig::new(APP_NAME)
            .description(env!("CARGO_PKG_DESCRIPTION"))
            .executable(std::env::current_exe().unwrap_or_else(|_| {
                std::env::args()
                    .next()
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| std::path::PathBuf::from(APP_NAME.to_lowercase()))
            }))
            .service_args(["daemon", "start"].into_iter().map(String::from).collect())
            .pid_dir(APP_DATA_DIR.join("logs").to_str().unwrap_or(".")),
    )
}

pub async fn run_daemon(background: bool) -> Result<()> {
    if !background {
        logging::init(true);
        println!("Starting sidedns daemon (foreground)...");
        sidedns_core::run(None).await?;
        return Ok(());
    }

    println!("Starting sidedns daemon...");

    let exe = std::env::current_exe()?;

    let mut cmd = tokio::process::Command::new(&exe);
    cmd.args(["daemon", "start", "--daemon"])
        .env(DAEMON_ENV, "1");

    #[cfg(windows)]
    {
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;

        cmd.creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    }

    cmd.spawn()?;

    println!("Waiting for daemon to start...");
    let client = IpcClient::default();

    for _ in 0..10 {
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        if client.is_running().await {
            println!("Daemon started");
            return Ok(());
        }
    }

    bail!("Daemon did not start within 10 seconds")
}

/// We're in a process that was spawned to run the daemon.
/// Don't attempt to spawn another daemon, just run it.
pub fn handle_daemon_start(args: &[String]) -> bool {
    let Ok(cli) = Cli::try_parse_from(args) else {
        return false;
    };
    if let Command::Daemon(args) = cli.command
        && let Some(DaemonAction::Start(args)) = args.action
        && std::env::var(DAEMON_ENV).is_ok()
        && args.background
        && args.daemon
    {
        let daemon = build_daemon();

        let result = daemon.start(false, || {
            logging::init(false);

            let rt = tokio::runtime::Runtime::new().map_err(daemon_kit::DaemonError::Io)?;

            rt.block_on(async { sidedns_core::run(None).await })
                .map_err(|e| daemon_kit::DaemonError::Service(e.to_string()))
        });

        if let Err(e) = result {
            eprintln!("Daemon error: {e}");
            std::process::exit(1);
        }
        return true;
    }
    false
}
