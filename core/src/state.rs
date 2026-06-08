use std::collections::HashSet;
use std::sync::Arc;

use crate::{DnsRule, RuleStore};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::ipc::DnsEvent;

/// The daemon's central state.
#[derive(Debug)]
pub struct AppState {
    pub store: RuleStore,
    pub events: broadcast::Sender<DnsEvent>,
    pub token: CancellationToken,
}

pub type SharedState = Arc<AppState>;

#[derive(Serialize, Deserialize, Default)]
pub struct DnsConfig {
    pub rules: Vec<DnsRule>,
}

impl AppState {
    pub(crate) fn new(token: CancellationToken) -> Self {
        let (events, _) = broadcast::channel(64);
        Self {
            store: RuleStore::new(),
            events,
            token,
        }
    }
}

impl AppState {
    pub fn dispatch(&self, event: DnsEvent) -> bool {
        self.events.send(event).is_ok()
    }

    pub fn snapshot_permanent(&self) -> Vec<DnsRule> {
        self.store.snapshot_persistent()
    }

    pub fn snapshot_all_domains(&self) -> Vec<String> {
        self.store
            .snapshot_all()
            .into_iter()
            .map(|r| r.domain)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn save(&self) {
        let config = DnsConfig {
            rules: self.snapshot_permanent(),
        };
        if let Err(e) = confy::store("sidedns", "rules", &config) {
            tracing::warn!("Failed to save rules: {e}");
        } else {
            tracing::info!("Rules saved");
        }
    }

    pub fn load_rules(&self) -> Vec<DnsRule> {
        let rules = match confy::load::<DnsConfig>("sidedns", "rules") {
            Ok(c) => c.rules,
            Err(e) => {
                tracing::warn!("Failed to load saved rules: {e}");
                vec![]
            },
        };
        let size = self.store.add_all(rules.clone());
        tracing::info!("Loaded {} saved rule(s)", size);
        rules
    }
}
