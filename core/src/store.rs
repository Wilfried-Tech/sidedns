use std::{collections::HashMap, sync::Arc};

use arc_swap::ArcSwap;

use crate::rule::DnsRule;

/// In-memory DNS rule store.
///
/// Resolution priority (first match wins):
/// 1. Ephemeral exact
/// 2. Ephemeral wildcard
/// 3. Persistent exact
/// 4. Persistent wildcard
///

#[derive(Debug, Default, Clone)]
pub struct InnerRuleStore {
    ephemeral_exact: HashMap<String, DnsRule>,
    persistent_exact: HashMap<String, DnsRule>,
    ephemeral_wildcard: Vec<DnsRule>,
    persistent_wildcard: Vec<DnsRule>,
}

impl InnerRuleStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve a domain to its matching rule.
    ///
    /// Checks ephemeral before persistent, exact before wildcard.
    pub fn resolve(&self, domain: &str) -> Option<&DnsRule> {
        if let Some(r) = self.ephemeral_exact.get(domain) {
            return Some(r);
        }
        if let Some(r) = self.find_wildcard(&self.ephemeral_wildcard, domain) {
            return Some(r);
        }
        if let Some(r) = self.persistent_exact.get(domain) {
            return Some(r);
        }
        if let Some(r) = self.find_wildcard(&self.persistent_wildcard, domain) {
            return Some(r);
        }
        None
    }

    /// Add or replace a persistent rule.
    pub fn add(&mut self, rule: DnsRule) {
        if rule.is_wildcard() {
            self.persistent_wildcard.retain(|r| r.domain != rule.domain);
            self.persistent_wildcard.push(rule);
        } else {
            self.persistent_exact.insert(rule.domain.clone(), rule);
        }
    }

    /// Add or replace persistent rules.
    pub fn add_all(&mut self, rules: Vec<DnsRule>) {
        rules.into_iter().for_each(|rule| self.add(rule));
    }

    /// Add or replace an ephemeral rule.
    pub fn add_ephemeral(&mut self, rule: DnsRule) {
        if rule.is_wildcard() {
            self.ephemeral_wildcard.retain(|r| r.domain != rule.domain);
            self.ephemeral_wildcard.push(rule);
        } else {
            self.ephemeral_exact.insert(rule.domain.clone(), rule);
        }
    }

    /// Remove the persistent rule for `domain`.
    pub fn remove(&mut self, domain: &str) -> Option<DnsRule> {
        let rule = self.persistent_exact.remove(domain);
        if let Some(pos) = self
            .persistent_wildcard
            .iter()
            .position(|x| x.domain == domain)
        {
            let item = self.persistent_wildcard.swap_remove(pos);
            return Some(item);
        }
        rule
    }

    /// Remove the ephemeral rule for `domain`.
    pub fn remove_ephemeral(&mut self, domain: &str) -> Option<DnsRule> {
        let rule = self.ephemeral_exact.remove(domain);
        if let Some(pos) = self
            .ephemeral_wildcard
            .iter()
            .position(|x| x.domain == domain)
        {
            let item = self.ephemeral_wildcard.swap_remove(pos);
            return Some(item);
        }
        rule
    }

    /// Iterate over persistent rules only.
    pub fn persistent_rules(&self) -> impl Iterator<Item = &DnsRule> {
        self.persistent_exact
            .values()
            .chain(self.persistent_wildcard.iter())
    }

    /// Iterate over all rules (ephemeral + persistent).
    pub fn all_rules(&self) -> impl Iterator<Item = &DnsRule> {
        self.ephemeral_exact
            .values()
            .chain(self.ephemeral_wildcard.iter())
            .chain(self.persistent_exact.values())
            .chain(self.persistent_wildcard.iter())
    }

    pub fn len(&self) -> usize {
        self.ephemeral_exact.len()
            + self.ephemeral_wildcard.len()
            + self.persistent_exact.len()
            + self.persistent_wildcard.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn find_wildcard<'a>(&self, list: &'a [DnsRule], domain: &str) -> Option<&'a DnsRule> {
        list.iter().find(|r| r.matches(domain))
    }
}

/// Concurrent, lock-free access to the [`InnerRuleStore`].
///
/// Reads are fully lock-free — they clone an `Arc` (~10ns) and read
/// from an immutable snapshot. Writers clone the current store, apply
/// the mutation, and swap atomically via [`ArcSwap::rcu`].
///
/// This is the correct abstraction for SideDNS because reads (DNS server,
/// HTTP proxy, WebSocket proxy) vastly outnumber writes (IPC add/remove).
///
/// # Cloning
///
/// [`RuleStore`] wraps an `Arc` internally — cloning it is cheap and
/// shares the same underlying data. All clones see writes immediately.
#[derive(Clone, Debug)]
pub struct RuleStore {
    inner: Arc<ArcSwap<InnerRuleStore>>,
}

impl Default for RuleStore {
    fn default() -> Self {
        Self {
            inner: Arc::new(ArcSwap::from_pointee(InnerRuleStore::new())),
        }
    }
}

impl RuleStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve a domain to its matching rule.
    ///
    /// Lock-free. Safe to call from hot paths (DNS, proxy).
    pub fn resolve(&self, domain: &str) -> Option<DnsRule> {
        self.inner.load().resolve(domain).cloned()
    }

    /// Add or replace a persistent rule.
    pub fn add(&self, rule: DnsRule) -> DnsRule {
        self.inner.rcu(|current| {
            let mut next = (**current).clone();
            next.add(rule.clone());
            next
        });
        rule
    }

    /// Add or replace persistent rules.
    pub fn add_all(&self, rules: Vec<DnsRule>) -> usize {
        self.inner.rcu(|current| {
            let mut next = (**current).clone();
            next.add_all(rules.clone());
            next
        });
        rules.len()
    }

    /// Add or replace an ephemeral rule.
    pub fn add_ephemeral(&self, rule: DnsRule) -> DnsRule {
        self.inner.rcu(|current| {
            let mut next = (**current).clone();
            next.add_ephemeral(rule.clone());
            next
        });
        rule
    }

    /// Remove the persistent rule for `domain`.
    ///
    /// Returns `true` if a rule was removed.
    pub fn remove(&self, domain: &str) -> Option<DnsRule> {
        let mut removed = None;
        self.inner.rcu(|current| {
            let mut next = (**current).clone();
            removed = next.remove(domain);
            next
        });
        removed
    }

    /// Remove the ephemeral rule for `domain`.
    ///
    /// Returns `true` if a rule was removed.
    pub fn remove_ephemeral(&self, domain: &str) -> Option<DnsRule> {
        let mut removed = None;
        self.inner.rcu(|current| {
            let mut next = (**current).clone();
            removed = next.remove_ephemeral(domain);
            next
        });
        removed
    }

    /// Return a snapshot of all persistent rules.
    ///
    /// Snapshot is consistent but may be stale by the time you use it.
    /// Only use for persistence (confy) and IPC list responses.
    pub fn snapshot_persistent(&self) -> Vec<DnsRule> {
        self.inner.load().persistent_rules().cloned().collect()
    }

    /// Return a snapshot of all rules (ephemeral + persistent).
    pub fn snapshot_all(&self) -> Vec<DnsRule> {
        self.inner.load().all_rules().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.inner.load().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.load().is_empty()
    }
}
