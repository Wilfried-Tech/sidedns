mod cli;
mod commands;
mod daemon;
mod run;
pub use daemon::handle_daemon_start;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use cli::{Cli, Command, DaemonAction};
use sidedns_core::IpcClient;

use crate::cli::{DEFAULT_DAEMON_START_ARGS, DaemonStartArgs};

async fn ensure_running() -> Result<()> {
    if !IpcClient::default().is_running().await {
        commands::daemon::start(DEFAULT_DAEMON_START_ARGS.clone()).await?;
    }
    Ok(())
}

/// Main entry point for the CLI.
pub async fn execute_from_command_line(args: Vec<String>) -> Result<()> {
    if args.len() == 1 {
        let mut cmd = Cli::command();
        cmd.print_long_help()?;
        std::process::exit(0);
    }

    let cli = Cli::try_parse_from(args).unwrap_or_else(|e| {
        use clap::error::ErrorKind;
        let _context = e.context().collect::<Vec<_>>();
        match e.kind() {
            ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                e.print().unwrap();
                std::process::exit(0);
            },
            _ => {
                e.print().unwrap();
                std::process::exit(1);
            },
        }
    });

    match cli.command {
        Command::Cert(args) => match args.action {
            cli::CertAction::Install(args) => commands::cert::install(args).await,
            cli::CertAction::Uninstall => commands::cert::uninstall().await,
            cli::CertAction::Trust(args) => commands::cert::trust(args).await,
            cli::CertAction::Untrust(args) => commands::cert::untrust(args).await,
        },

        Command::Add(args) => {
            ensure_running().await?;
            commands::add::run(args).await
        },

        Command::Remove(args) => {
            ensure_running().await?;
            commands::remove::run(args).await
        },

        Command::List => {
            ensure_running().await?;
            commands::list::run().await
        },

        Command::Resolve(args) => {
            ensure_running().await?;
            commands::resolve::run(args).await
        },

        Command::Status => commands::status::run().await,

        Command::Daemon(args) => match args
            .action
            .unwrap_or(DaemonAction::Start(DEFAULT_DAEMON_START_ARGS.clone()))
        {
            DaemonAction::Start(args) => commands::daemon::start(args).await,
            DaemonAction::Stop => commands::daemon::stop().await,
        },

        Command::Run(args) => {
            ensure_running().await?;
            commands::run::run(args).await
        },
        Command::Watch => {
            ensure_running().await?;
            commands::watch::run().await
        },
        Command::Clean => commands::clean::run().await,
    }
}
