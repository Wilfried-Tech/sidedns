pub mod certs;
mod config;
mod dns;
pub mod ipc;
pub mod logging;
mod proxy;
mod rule;
mod runner;
mod state;
pub mod store;

pub use config::*;
pub use dns::platform_dns_manager;
pub use ipc::{DnsEvent, IpcClient, IpcRequest, IpcResponse};
pub use logging::init as init_logging;
pub use rule::DnsRule;
pub use runner::run;
pub use state::{AppState, SharedState};
pub use store::RuleStore;

pub fn validate_domain(domain: &str) -> bool {
    domain.len() < 253 && DOMAIN_REGEX.is_match(domain)
}
