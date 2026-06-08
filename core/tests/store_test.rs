use sidedns_core::{store::InnerRuleStore, *};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

fn rule(domain: &str, port: u16) -> DnsRule {
    DnsRule::new(
        domain.to_string(),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
        false,
    )
}

#[test]
fn exact_persistent_resolves() {
    let mut store = InnerRuleStore::new();
    store.add(rule("api.local", 3000));
    assert_eq!(
        store.resolve("api.local").map(|r| r.target.port()),
        Some(3000)
    );
}

#[test]
fn unknown_returns_none() {
    let store = InnerRuleStore::new();
    assert!(store.resolve("ghost.local").is_none());
}

#[test]
fn ephemeral_takes_priority_over_persistent() {
    let mut store = InnerRuleStore::new();
    store.add(rule("api.local", 3000));
    store.add_ephemeral(rule("api.local", 9000));
    assert_eq!(
        store.resolve("api.local").map(|r| r.target.port()),
        Some(9000)
    );
}

#[test]
fn remove_ephemeral_falls_back_to_persistent() {
    let mut store = InnerRuleStore::new();
    store.add(rule("api.local", 3000));
    store.add_ephemeral(rule("api.local", 9000));
    store.remove_ephemeral("api.local");
    assert_eq!(
        store.resolve("api.local").map(|r| r.target.port()),
        Some(3000)
    );
}

#[test]
fn wildcard_matches_subdomain() {
    let mut store = InnerRuleStore::new();
    store.add(rule("*.local", 4000));
    assert_eq!(
        store.resolve("api.local").map(|r| r.target.port()),
        Some(4000)
    );
    assert_eq!(
        store.resolve("auth.local").map(|r| r.target.port()),
        Some(4000)
    );
}

#[test]
fn wildcard_does_not_match_root() {
    let mut store = InnerRuleStore::new();
    store.add(rule("*.local", 4000));
    assert!(store.resolve("local").is_none());
}

#[test]
fn exact_beats_wildcard() {
    let mut store = InnerRuleStore::new();
    store.add(rule("*.local", 4000));
    store.add(rule("api.local", 3000));
    assert_eq!(
        store.resolve("api.local").map(|r| r.target.port()),
        Some(3000)
    );
}

#[test]
fn ephemeral_wildcard_beats_persistent_exact() {
    let mut store = InnerRuleStore::new();
    store.add_ephemeral(rule("*.local", 9000));
    store.add(rule("api.local", 3000));
    assert_eq!(
        store.resolve("api.local").map(|r| r.target.port()),
        Some(9000)
    );
}

#[test]
fn persistent_wildcard_matches_after_exact_miss() {
    let mut store = InnerRuleStore::new();
    store.add(rule("*.local", 4000));
    store.add(rule("api.local", 3000));
    assert_eq!(
        store.resolve("auth.local").map(|r| r.target.port()),
        Some(4000)
    );
}

#[test]
fn remove_returns_none_for_unknown() {
    let mut store = InnerRuleStore::new();
    assert!(store.remove("unknown.local").is_none());
}

#[test]
fn add_replaces_existing_persistent() {
    let mut store = InnerRuleStore::new();
    store.add(rule("api.local", 3000));
    store.add(rule("api.local", 5000));
    assert_eq!(
        store.resolve("api.local").map(|r| r.target.port()),
        Some(5000)
    );
}

#[test]
fn remove_persistent_keeps_ephemeral() {
    let mut store = InnerRuleStore::new();
    store.add(rule("api.local", 3000));
    store.add_ephemeral(rule("api.local", 9000));
    store.remove("api.local");
    assert_eq!(
        store.resolve("api.local").map(|r| r.target.port()),
        Some(9000)
    );
}

#[test]
fn inner_store_is_empty_initially() {
    let store = InnerRuleStore::new();
    assert!(store.is_empty());
    assert_eq!(store.len(), 0);
}

#[test]
fn inner_store_len_counts_all_layers() {
    let mut store = InnerRuleStore::new();
    store.add(rule("api.local", 3000));
    store.add(rule("*.local", 4000));
    store.add_ephemeral(rule("tmp.local", 9000));
    assert_eq!(store.len(), 3);
    assert!(!store.is_empty());
}

#[test]
fn inner_store_remove_reduces_len() {
    let mut store = InnerRuleStore::new();
    store.add(rule("api.local", 3000));
    store.remove("api.local");
    assert_eq!(store.len(), 0);
    assert!(store.is_empty());
}

#[test]
fn inner_store_all_rules_includes_ephemeral() {
    let mut store = InnerRuleStore::new();
    store.add(rule("api.local", 3000));
    store.add_ephemeral(rule("tmp.local", 9000));
    let all: Vec<_> = store.all_rules().collect();
    assert_eq!(all.len(), 2);
}

#[test]
fn inner_store_persistent_rules_excludes_ephemeral() {
    let mut store = InnerRuleStore::new();
    store.add(rule("api.local", 3000));
    store.add_ephemeral(rule("tmp.local", 9000));
    let persistent: Vec<_> = store.persistent_rules().collect();
    assert_eq!(persistent.len(), 1);
    assert_eq!(persistent[0].domain, "api.local");
}

#[test]
fn inner_store_remove_wildcard_clears_wildcard_not_exact() {
    let mut store = InnerRuleStore::new();
    store.add(rule("*.local", 4000));
    store.add(rule("api.local", 3000));
    store.remove("*.local");
    // exact still resolves
    assert_eq!(
        store.resolve("api.local").map(|r| r.target.port()),
        Some(3000)
    );
    // other subdomain no longer resolves via wildcard
    assert!(store.resolve("auth.local").is_none());
}

#[test]
fn inner_store_ephemeral_wildcard_beats_persistent_wildcard() {
    let mut store = InnerRuleStore::new();
    store.add(rule("*.local", 4000));
    store.add_ephemeral(rule("*.local", 9000));
    assert_eq!(
        store.resolve("api.local").map(|r| r.target.port()),
        Some(9000)
    );
}

#[test]
fn inner_store_remove_ephemeral_wildcard_falls_back_to_persistent_wildcard() {
    let mut store = InnerRuleStore::new();
    store.add(rule("*.local", 4000));
    store.add_ephemeral(rule("*.local", 9000));
    store.remove_ephemeral("*.local");
    assert_eq!(
        store.resolve("api.local").map(|r| r.target.port()),
        Some(4000)
    );
}

#[test]
fn add_and_resolve() {
    let rules = RuleStore::new();
    rules.add(rule("api.local", 3000));
    assert_eq!(
        rules.resolve("api.local").map(|r| r.target.port()),
        Some(3000)
    );
}

#[test]
fn clone_shares_state() {
    let rules = RuleStore::new();
    let clone = rules.clone();
    rules.add(rule("api.local", 3000));
    assert_eq!(
        clone.resolve("api.local").map(|r| r.target.port()),
        Some(3000)
    );
}

#[test]
fn remove_returns_some_for_existing() {
    let rules = RuleStore::new();
    rules.add(rule("api.local", 3000));
    assert!(rules.remove("api.local").is_some());
    assert!(rules.resolve("api.local").is_none());
}

#[test]
fn rule_store_remove_returns_none_for_unknown() {
    let rules = RuleStore::new();
    assert!(rules.remove("ghost.local").is_none());
}

#[test]
fn ephemeral_priority() {
    let rules = RuleStore::new();
    rules.add(rule("api.local", 3000));
    rules.add_ephemeral(rule("api.local", 9000));
    assert_eq!(
        rules.resolve("api.local").map(|r| r.target.port()),
        Some(9000)
    );
}

#[test]
fn remove_ephemeral_falls_back() {
    let rules = RuleStore::new();
    rules.add(rule("api.local", 3000));
    rules.add_ephemeral(rule("api.local", 9000));
    rules.remove_ephemeral("api.local");
    assert_eq!(
        rules.resolve("api.local").map(|r| r.target.port()),
        Some(3000)
    );
}

#[test]
fn wildcard_resolves() {
    let rules = RuleStore::new();
    rules.add(rule("*.local", 4000));
    assert_eq!(
        rules.resolve("api.local").map(|r| r.target.port()),
        Some(4000)
    );
    assert_eq!(
        rules.resolve("auth.local").map(|r| r.target.port()),
        Some(4000)
    );
}

#[test]
fn snapshot_persistent_excludes_ephemeral() {
    let rules = RuleStore::new();
    rules.add(rule("api.local", 3000));
    rules.add_ephemeral(rule("tmp.local", 9000));
    let snap = rules.snapshot_persistent();
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].domain, "api.local");
}

#[test]
fn concurrent_reads_and_write() {
    use std::thread;
    let rules = RuleStore::new();
    rules.add(rule("api.local", 3000));

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let r = rules.clone();
            thread::spawn(move || {
                for _ in 0..1000 {
                    let _ = r.resolve("api.local");
                }
            })
        })
        .collect();

    rules.add(rule("api.local", 5000));

    for h in handles {
        h.join().unwrap();
    }

    let port = rules.resolve("api.local").map(|r| r.target.port());
    assert!(matches!(port, Some(3000) | Some(5000)));
}

#[test]
fn rule_store_len_and_is_empty() {
    let store = RuleStore::new();
    assert!(store.is_empty());
    store.add(rule("api.local", 3000));
    assert_eq!(store.len(), 1);
    assert!(!store.is_empty());
}

#[test]
fn rule_store_snapshot_all_includes_ephemeral() {
    let store = RuleStore::new();
    store.add(rule("api.local", 3000));
    store.add_ephemeral(rule("tmp.local", 9000));
    assert_eq!(store.snapshot_all().len(), 2);
}

#[test]
fn rule_store_remove_ephemeral_falls_back() {
    let store = RuleStore::new();
    store.add(rule("api.local", 3000));
    store.add_ephemeral(rule("api.local", 9000));
    store.remove_ephemeral("api.local");
    assert_eq!(
        store.resolve("api.local").map(|r| r.target.port()),
        Some(3000)
    );
}

#[test]
fn rule_store_add_all_bulk_inserts() {
    let store = RuleStore::new();
    let rules = vec![
        rule("api.local", 3000),
        rule("auth.local", 4000),
        rule("*.internal", 5000),
    ];
    store.add_all(rules);
    assert_eq!(store.len(), 3);
    assert!(store.resolve("api.local").is_some());
    assert!(store.resolve("svc.internal").is_some());
}
