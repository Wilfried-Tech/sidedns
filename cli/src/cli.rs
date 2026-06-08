use std::net::IpAddr;

use clap::{Args, Parser, Subcommand, crate_authors, crate_description, crate_version};

#[derive(Parser)]
#[command(
    name = "sidedns",
    author = crate_authors!("\n"),
    version = crate_version!(),
    about = crate_description!(),
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Cert(CertArgs),
    /// List all active DNS routing rules
    List,
    Add(AddArgs),
    Remove(RemoveArgs),
    Resolve(ResolveArgs),
    Daemon(DaemonArgs),
    Run(RunArgs),
    /// Show daemon status
    Status,
    /// Clean all Sidedns system dns rules
    Clean,
    /// Watch for rule changes and print them in real time
    Watch,
}

/// Manage the root CA certificate used for HTTPS proxying
#[derive(Args)]
pub struct CertArgs {
    #[command(subcommand)]
    pub action: CertAction,
}

#[derive(Subcommand)]
pub enum CertAction {
    /// Install the root CA certificate (if not already installed)
    Install(CertInstallArgs),
    /// Uninstall the root CA certificate (if installed)
    Uninstall,
    /// Trust the root CA in all supported stores (requires admin)
    Trust(CertTrustArgs),
    /// Untrust the root CA in all supported stores (requires admin)
    Untrust(CertTrustArgs),
}

#[derive(Args)]
pub struct CertInstallArgs {
    /// Force re-generation and re-installation of the root CA, replacing any existing one
    #[arg(short, long)]
    pub force: bool,
    /// Trust the root CA in all supported stores (requires admin)
    #[arg(short, long)]
    pub trust: bool,
}

#[derive(Args, Clone)]
pub struct CertTrustArgs {
    /// All supported stores
    #[arg(short, long, default_value_t = true)]
    pub all: bool,
    /// The system store
    #[arg(short, long, overrides_with = "all")]
    pub system: bool,
    /// The Java store
    #[arg(short, long, overrides_with = "all")]
    pub java: bool,
    /// The NSS store
    #[arg(short, long, overrides_with = "all")]
    pub nss: bool,
}

pub static CERT_TRUST_DEFAULT_ARGS: CertTrustArgs = CertTrustArgs {
    all: true,
    system: false,
    java: false,
    nss: false,
};

/// Add or replace a DNS routing rule
///
/// Examples:
///
/// sidedns add api.local --ip 127.0.0.1 --port 3000 --https
#[derive(Args)]
pub struct AddArgs {
    /// Domain name to route (e.g. "api.local")
    #[arg(value_parser = validate_domain)]
    pub domain: String,
    /// Target IP address (e.g. "127.0.0.1")
    #[arg(short, long, default_value = "127.0.0.1")]
    pub ip: IpAddr,
    /// Target port (e.g. 3000)
    #[arg(short, long, default_value = "80")]
    pub port: u16,
    /// Enable HTTPS TLS proxy for this rule
    #[arg(long)]
    pub https: bool,
}

/// Remove a DNS routing rule
#[derive(Args)]
pub struct RemoveArgs {
    #[arg(value_parser = validate_domain)]
    pub domain: String,
}

/// Resolve a domain to its configured target
#[derive(Args)]
pub struct ResolveArgs {
    #[arg(value_parser = validate_domain_resolve)]
    pub domain: String,
}

/// Manage the background daemon
#[derive(Args)]
pub struct DaemonArgs {
    #[command(subcommand)]
    pub action: Option<DaemonAction>,
}

#[derive(Subcommand)]
pub enum DaemonAction {
    Start(DaemonStartArgs),
    /// Stop the running daemon.
    Stop,
}

/// Start the daemon in the background
///
/// Use `--no-background` to run in the foreground instead (for debugging)
#[derive(Args, Clone)]
pub struct DaemonStartArgs {
    /// Run the daemon in background (detached from the terminal)
    #[arg(
        long,
        short = 'd',
        overrides_with = "no_background",
        default_value_t = true
    )]
    pub background: bool,
    /// Run the daemon in the foreground (attached to the terminal)
    #[arg(long, overrides_with = "background")]
    pub no_background: bool,
    /// Internal flag to indicate the daemon should start itself (not via CLI command)
    /// in combination with env DAEMON_PROCESS=1
    #[arg(long, hide = true)]
    pub daemon: bool,
}

pub static DEFAULT_DAEMON_START_ARGS: DaemonStartArgs = DaemonStartArgs {
    background: true,
    no_background: false,
    daemon: false,
};

/// Run a command with an ephemeral DNS routing rule active for the duration of its execution.
#[derive(Args)]
#[command(after_help = "\
\x1b[1m\x1b[4mExamples:\x1b[0m\n\
\tsidedns run -d example.local --ip 127.0.0.1 --port 8000 --https\n\
\tsidedns run -d example.com fastapi dev # detect port automatically\n
")]
pub struct RunArgs {
    /// Domain name to route (e.g. "example.local")
    #[arg(short, long, value_parser = validate_domain)]
    pub domain: String,
    /// Target IP address — if omitted, only ipv4 auto-detected after launch
    #[arg(long, short)]
    pub ip: Option<IpAddr>,
    /// Target port — if omitted, auto-detected after launch
    #[arg(long, short)]
    pub port: Option<u16>,
    /// Enable HTTPS TLS proxy for this ephemeral rule
    #[arg(long, short('s'))]
    pub https: bool,
    /// number of seconds to wait before detecting port
    #[arg(short = 'w', long = "wait", default_value_t = 5)]
    pub detect_timeout: u32,
    /// The command to run, followed by its arguments
    #[arg(
        trailing_var_arg = true,
        num_args = 1..,
        required = true,
    )]
    pub command: Vec<String>,
}

fn validate_domain(s: &str) -> anyhow::Result<String> {
    if sidedns_core::validate_domain(s) {
        Ok(s.to_lowercase())
    } else {
        Err(anyhow::anyhow!("Invalid domain name"))
    }
}

fn validate_domain_resolve(s: &str) -> anyhow::Result<String> {
    match validate_domain(s) {
        Ok(domain) => {
            if domain.starts_with("*.") {
                Err(anyhow::anyhow!(
                    "Wildcard domains are not allowed, please provide an explicit domain, Ex: {}",
                    domain.replace("*.", "example.")
                ))
            } else {
                Ok(domain)
            }
        },
        err => err,
    }
}
