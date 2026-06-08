use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::DnsRule;
use crate::ipc::message::{DnsEvent, IpcRequest, IpcResponse};
use crate::ipc::server::IpcHandler;
use crate::state::AppState;

macro_rules! validate_domain {
    ($domain:ident) => {
        if !crate::validate_domain(&$domain) {
            tracing::warn!("Invalid domain name: {}", $domain);
            return IpcResponse::Error(format!("Invalid domain name: {}", $domain));
        }
        let $domain = $domain.to_lowercase();
    };
}

#[async_trait]
impl IpcHandler for AppState {
    async fn handle(&self, request: IpcRequest) -> IpcResponse {
        match request {
            IpcRequest::Add {
                domain,
                target,
                https,
            } => {
                validate_domain!(domain);
                let rule = self.store.add(DnsRule::new(domain.clone(), target, https));
                tracing::info!("Rule added: {rule}");
                self.events.send(DnsEvent::RuleAdded(rule)).ok();
                IpcResponse::Ok
            },

            IpcRequest::AddEphemeral {
                domain,
                target,
                https,
            } => {
                validate_domain!(domain);
                let rule = self
                    .store
                    .add_ephemeral(DnsRule::new(domain.clone(), target, https));
                tracing::info!("Ephemeral rule added: {rule}");
                self.events.send(DnsEvent::EphemeralAdded(rule)).ok();
                IpcResponse::Ok
            },

            IpcRequest::Remove { domain } => {
                validate_domain!(domain);
                let removed = self.store.remove(&domain);
                if let Some(rule) = removed {
                    self.events.send(DnsEvent::RuleRemoved(rule)).ok();
                    tracing::info!("Rule removed");
                    IpcResponse::Ok
                } else {
                    IpcResponse::Error(format!("No rule found for '{domain}'"))
                }
            },

            IpcRequest::RemoveEphemeral { domain } => {
                validate_domain!(domain);
                let removed = self.store.remove_ephemeral(&domain);
                if let Some(rule) = removed {
                    self.events.send(DnsEvent::EphemeralRemoved(rule)).ok();
                    tracing::info!("Ephemeral rule removed");
                    IpcResponse::Ok
                } else {
                    IpcResponse::Error(format!("No ephemeral rule found for '{domain}'"))
                }
            },

            IpcRequest::List => {
                let rules = self.store.snapshot_persistent();
                IpcResponse::Rules(rules)
            },

            IpcRequest::Resolve { domain } => {
                validate_domain!(domain);
                IpcResponse::Resolved(self.store.resolve(&domain))
            },

            IpcRequest::Status => IpcResponse::Status {
                running: true,
                rule_count: self.store.len(),
            },

            IpcRequest::Stop => {
                tracing::info!("Stop requested via IPC");
                self.events.send(DnsEvent::DaemonStopped).ok();
                self.token.cancel();
                IpcResponse::Ok
            },
            IpcRequest::Subscribe => IpcResponse::Ok,
        }
    }

    fn subscribe_events(&self) -> broadcast::Receiver<DnsEvent> {
        self.events.subscribe()
    }
}
