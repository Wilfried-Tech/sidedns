use std::net::SocketAddr;

use crate::DnsRule;
use serde::{Deserialize, Serialize};

/// Commands sent from CLI or GUI to the daemon over IPC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", content = "payload")]
pub enum IpcRequest {
    /// Add or replace a DNS routing rule.
    Add {
        domain: String,
        target: SocketAddr,
        https: bool,
    },
    /// Add an ephemeral rule (not persisted, takes priority over permanent rules).
    AddEphemeral {
        domain: String,
        target: SocketAddr,
        https: bool,
    },
    /// Remove the permanent rule for the given domain.
    Remove { domain: String },
    /// Remove the ephemeral rule for the given domain.
    RemoveEphemeral { domain: String },
    /// List all active rules.
    List,
    /// Resolve a domain to its configured target.
    Resolve { domain: String },
    /// Query daemon status.
    Status,
    /// Request a graceful shutdown of the daemon.
    Stop,
    /// Open a persistent event stream connection.
    Subscribe,
}

/// Responses sent from the daemon to a client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum IpcResponse {
    Ok,
    Error(String),
    Rules(Vec<DnsRule>),
    Resolved(Option<DnsRule>),
    Status { running: bool, rule_count: usize },
    Event(DnsEvent),
}

/// Events emitted by the daemon and pushed to subscribed clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DnsEvent {
    /// A rule was added or replaced.
    RuleAdded(DnsRule),
    /// A rule was removed.
    RuleRemoved(DnsRule),
    /// An ephemeral rule was added.
    EphemeralAdded(DnsRule),
    /// An ephemeral rule was removed.
    EphemeralRemoved(DnsRule),
    DaemonStopped,
}
