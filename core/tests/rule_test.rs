use sidedns_core::DnsRule;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

fn addr(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}
fn rule(domain: &str) -> DnsRule {
    DnsRule::new(domain.to_string(), addr(3000), false)
}

#[test]
fn is_wildcard_star_dot() {
    assert!(rule("*.local").is_wildcard());
}
#[test]
fn is_wildcard_nested() {
    assert!(rule("*.api.example.com").is_wildcard());
}
#[test]
fn is_wildcard_false_for_exact() {
    assert!(!rule("api.local").is_wildcard());
}
#[test]
fn is_wildcard_false_for_localhost() {
    assert!(!rule("localhost").is_wildcard());
}

#[test]
fn exact_matches_itself() {
    assert!(rule("api.local").matches("api.local"));
}
#[test]
fn exact_no_match_other() {
    assert!(!rule("api.local").matches("auth.local"));
}
#[test]
fn exact_case_sensitive() {
    assert!(!rule("API.local").matches("api.local"));
}

#[test]
fn wildcard_matches_subdomain() {
    assert!(rule("*.local").matches("api.local"));
}
#[test]
fn wildcard_matches_deep() {
    assert!(rule("*.local").matches("a.b.local"));
}
#[test]
fn wildcard_no_match_root() {
    assert!(!rule("*.local").matches("local"));
}
#[test]
fn wildcard_no_match_other_tld() {
    assert!(!rule("*.local").matches("api.remote"));
}
#[test]
fn wildcard_no_partial_suffix() {
    assert!(!rule("*.local").matches("notlocal"));
}
#[test]
fn wildcard_requires_dot() {
    assert!(!rule("*.local").matches("apilocal"));
}

#[test]
fn display_http() {
    let s = DnsRule::new("api.local".into(), addr(3000), false).to_string();
    assert!(s.contains("http://") && s.contains("api.local") && s.contains("3000"));
}
#[test]
fn display_https() {
    let s = DnsRule::new("api.local".into(), addr(443), true).to_string();
    assert!(s.contains("https://"));
}

#[test]
fn with_https_enables() {
    assert!(rule("api.local").with_https(true).https);
}
#[test]
fn with_https_disables() {
    assert!(
        !DnsRule::new("api.local".into(), addr(443), true)
            .with_https(false)
            .https
    );
}

#[test]
fn equal_same_fields() {
    assert_eq!(
        DnsRule::new("api.local".into(), addr(3000), false),
        DnsRule::new("api.local".into(), addr(3000), false)
    );
}
#[test]
fn ne_different_port() {
    assert_ne!(
        DnsRule::new("api.local".into(), addr(3000), false),
        DnsRule::new("api.local".into(), addr(4000), false)
    );
}
#[test]
fn ne_different_https() {
    assert_ne!(
        DnsRule::new("api.local".into(), addr(3000), false),
        DnsRule::new("api.local".into(), addr(3000), true)
    );
}
#[test]
fn clone_equal() {
    let r = DnsRule::new("api.local".into(), addr(3000), false);
    assert_eq!(r.clone(), r);
}
