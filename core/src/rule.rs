use serde::{Deserialize, Serialize};
use std::{fmt::Display, net::SocketAddr};

/// A single DNS routing rule mapping a domain to a target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsRule {
    pub domain: String,
    pub target: SocketAddr,
    /// Whether SideDNS should terminate TLS for this domain and proxy HTTPS.
    pub https: bool,
}

impl DnsRule {
    pub fn is_wildcard(&self) -> bool {
        self.domain.strip_prefix("*.").is_some()
    }

    pub fn matches(&self, domain: &str) -> bool {
        match self.domain.strip_prefix("*.") {
            Some(suffix) => {
                domain.ends_with(suffix)
                    && domain.len() > suffix.len()
                    && domain.as_bytes()[domain.len() - suffix.len() - 1] == b'.'
            },
            None => self.domain == domain,
        }
    }
    pub fn new(domain: String, target: SocketAddr, https: bool) -> Self {
        Self {
            domain,
            target,
            https,
        }
    }
    pub fn with_https(mut self, https: bool) -> Self {
        self.https = https;
        self
    }
}

impl Display for DnsRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let scheme = if self.https { "https" } else { "http" };
        write!(f, "{scheme}://{} → {}", self.domain, self.target)
    }
}
