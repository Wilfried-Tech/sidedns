use std::sync::Arc;

use dashmap::DashMap;
use tokio_rustls::rustls::{
    server::{ClientHello, ResolvesServerCert},
    sign::CertifiedKey,
};

use crate::{DnsRule, SharedState, certs::Ca};

#[derive(Debug)]
pub struct DomainResolver {
    state: SharedState,
    cert_authority: Option<Ca>,
    cache: DashMap<String, DnsRule>,
    certs_cache: DashMap<String, Arc<CertifiedKey>>,
}

impl DomainResolver {
    pub fn new(state: SharedState, cert_authority: Option<Ca>) -> Self {
        Self {
            state,
            cert_authority,
            cache: DashMap::new(),
            certs_cache: DashMap::new(),
        }
    }

    pub fn is_cert_installed(&self) -> bool {
        self.cert_authority.is_some() && Ca::is_installed()
    }

    pub fn resolve_domain(&self, domain: &str) -> Option<DnsRule> {
        if let Some(rule) = self.cache.get(domain) {
            return Some(rule.clone());
        }
        let rule = self.state.store.resolve(domain)?;
        self.cache.insert(domain.to_owned(), rule.clone());
        Some(rule)
    }

    pub fn sign_domain(&self, domain: &str) -> Option<Arc<CertifiedKey>> {
        let Some(ca) = &self.cert_authority else {
            tracing::warn!("Certificate requested for {domain}, but CA is not available");
            return None;
        };
        let cert = Arc::new(ca.sign(&[domain]).ok()?);

        self.certs_cache.insert(domain.to_owned(), cert.clone());

        Some(cert)
    }

    pub fn invalidate(&self, rule: &DnsRule) {
        if let Some(suffix) = rule.domain.strip_prefix("*.") {
            let retain = |key: &str| {
                !(key.ends_with(suffix)
                    && key.len() > suffix.len()
                    && key.as_bytes()[key.len() - suffix.len() - 1] == b'.')
            };
            self.cache.retain(|key, _| retain(key));
            self.certs_cache.retain(|key, _| retain(key));
        } else {
            self.cache.remove(rule.domain.as_str());
            self.certs_cache.remove(rule.domain.as_str());
        }
    }
}

impl ResolvesServerCert for DomainResolver {
    #[tracing::instrument(skip(self, client_hello), name = "ResolveCert")]
    fn resolve(&self, client_hello: ClientHello) -> Option<Arc<CertifiedKey>> {
        let domain = client_hello.server_name()?;

        if self.cert_authority.is_none() {
            tracing::warn!("Certificate requested for {domain}, but CA is not available");
            return None;
        };

        if !self.resolve_domain(domain).is_some_and(|r| r.https) {
            tracing::info!(
                "Certificate requested for {domain}, but domain is not configured for HTTPS"
            );
            return None;
        }

        if let Some(cert) = self.certs_cache.get(domain) {
            return Some(cert.clone());
        }

        let cert = self.sign_domain(domain)?;

        Some(cert)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        net::{IpAddr, Ipv4Addr, SocketAddr},
        sync::Arc,
    };
    use tokio_util::sync::CancellationToken;

    use crate::{AppState, DnsRule};

    fn addr(port: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
    }

    fn rule(domain: &str, port: u16) -> DnsRule {
        DnsRule::new(domain.to_string(), addr(port), false)
    }

    fn rule_https(domain: &str, port: u16) -> DnsRule {
        DnsRule::new(domain.to_string(), addr(port), true)
    }

    fn make_resolver(ca: Option<Ca>) -> (DomainResolver, SharedState) {
        let token = CancellationToken::new();
        let state = Arc::new(AppState::new(token));
        let resolver = DomainResolver::new(state.clone(), ca);
        (resolver, state)
    }

    fn make_resolver_with_ca() -> (DomainResolver, SharedState) {
        let token = CancellationToken::new();
        let state = Arc::new(AppState::new(token));
        let (_, ca) = Ca::generate().unwrap();
        let resolver = DomainResolver::new(state.clone(), Some(ca));
        (resolver, state)
    }

    #[test]
    fn resolve_populates_cache_on_miss() {
        let (resolver, state) = make_resolver(None);
        state.store.add(rule("api.local", 3000));

        let result = resolver.resolve_domain("api.local");
        assert_eq!(result.map(|r| r.target.port()), Some(3000));

        assert!(resolver.cache.contains_key("api.local"));
    }

    #[test]
    fn resolve_cache_hit_returns_same_value() {
        let (resolver, state) = make_resolver(None);
        state.store.add(rule("api.local", 3000));

        resolver.resolve_domain("api.local");

        state.store.add(rule("api.local", 9999));

        assert_eq!(
            resolver
                .resolve_domain("api.local")
                .map(|r| r.target.port()),
            Some(3000)
        );
    }

    #[test]
    fn resolve_returns_none_for_unknown() {
        let (resolver, _state) = make_resolver(None);
        assert!(resolver.resolve_domain("ghost.local").is_none());
    }

    #[test]
    fn invalidate_removes_exact_domain_from_cache() {
        let (resolver, state) = make_resolver(None);
        state.store.add(rule("api.local", 3000));

        resolver.resolve_domain("api.local");
        assert!(resolver.cache.contains_key("api.local"));

        resolver.invalidate(&rule("api.local", 3000));
        assert!(!resolver.cache.contains_key("api.local"));
    }

    #[test]
    fn invalidate_removes_exact_domain_from_certs_cache() {
        let (resolver, state) = make_resolver_with_ca();

        state.store.add(rule_https("api.local", 3000));
        resolver.sign_domain("api.local");

        assert!(resolver.resolve_domain("api.local").is_some());
        assert!(resolver.certs_cache.contains_key("api.local"));

        resolver.invalidate(&rule_https("api.local", 3000));

        assert!(!resolver.certs_cache.contains_key("api.local"));
        assert!(!resolver.cache.contains_key("api.local"));
    }

    #[test]
    fn invalidate_exact_does_not_remove_other_domains() {
        let (resolver, state) = make_resolver(None);
        state.store.add(rule("api.local", 3000));
        state.store.add(rule("auth.local", 4000));

        resolver.resolve_domain("api.local");
        resolver.resolve_domain("auth.local");

        resolver.invalidate(&rule("api.local", 3000));

        assert!(!resolver.cache.contains_key("api.local"));
        assert!(resolver.cache.contains_key("auth.local"));
    }

    #[test]
    fn invalidate_wildcard_removes_matching_cached_entries() {
        let (resolver, state) = make_resolver(None);
        state.store.add(rule("*.local", 4000));

        resolver.resolve_domain("api.local");
        resolver.resolve_domain("auth.local");
        assert!(resolver.cache.contains_key("api.local"));
        assert!(resolver.cache.contains_key("auth.local"));

        resolver.invalidate(&rule("*.local", 4000));

        assert!(!resolver.cache.contains_key("api.local"));
        assert!(!resolver.cache.contains_key("auth.local"));
    }

    #[test]
    fn invalidate_wildcard_does_not_remove_non_matching() {
        let (resolver, state) = make_resolver(None);
        state.store.add(rule("api.remote", 5000));

        resolver.resolve_domain("api.remote");
        assert!(resolver.cache.contains_key("api.remote"));

        resolver.invalidate(&rule("*.local", 4000));

        assert!(resolver.cache.contains_key("api.remote"));
    }

    #[test]
    fn invalidate_wildcard_does_not_remove_root_domain() {
        let (resolver, state) = make_resolver(None);
        state.store.add(rule("local", 5000));

        resolver.resolve_domain("local");
        assert!(resolver.cache.contains_key("local"));

        resolver.invalidate(&rule("*.local", 4000));

        assert!(resolver.cache.contains_key("local"));
    }

    #[test]
    fn sign_domain_returns_none_when_no_ca() {
        let (resolver, _state) = make_resolver(None);
        assert!(resolver.sign_domain("api.local").is_none());
    }

    #[test]
    fn sign_domain_returns_cert_and_caches_it() {
        let (resolver, _state) = make_resolver_with_ca();
        let cert = resolver.sign_domain("api.local");
        assert!(cert.is_some());

        assert!(resolver.certs_cache.contains_key("api.local"));
    }

    #[test]
    fn sign_domain_caches_cert_on_repeated_calls() {
        let (resolver, _state) = make_resolver_with_ca();

        resolver.sign_domain("api.local");
        resolver.sign_domain("api.local");

        assert_eq!(resolver.certs_cache.len(), 1);
    }
}
