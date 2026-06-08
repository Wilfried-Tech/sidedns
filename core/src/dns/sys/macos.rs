use std::path::{Path, PathBuf};

use super::SystemDnsManager;

const RESOLVER_DIR: &str = "/etc/resolver";

/// Configures split DNS via `/etc/resolver/<domain>` files on macOS.
///
/// Creates one resolver file per domain. The macOS DNS resolver reads these
/// automatically — no reload command needed. Removes all SideDNS-managed
/// files on revert.
///
/// Requires write access to `/etc/resolver/` (root).
pub struct MacOsConfigurator;

impl MacOsConfigurator {
    fn resolver_path(domain: &str) -> PathBuf {
        Path::new(RESOLVER_DIR).join(domain)
    }

    fn resolver_content() -> String {
        let dns_ip = super::DNS_LISTEN_ADDR.ip();
        format!(
            "# Managed by SideDNS — do not edit manually\n\
             nameserver {dns_ip}\n"
        )
    }

    fn is_sidedns_file(path: &Path) -> bool {
        std::fs::read_to_string(path)
            .map(|c| c.contains("Managed by SideDNS"))
            .unwrap_or(false)
    }

    fn remove_all_files() -> anyhow::Result<()> {
        if let Ok(entries) = std::fs::read_dir(RESOLVER_DIR) {
            for entry in entries.flatten() {
                let path = entry.path();
                if Self::is_sidedns_file(&path) {
                    std::fs::remove_file(&path)?;
                }
            }
        }
        Ok(())
    }
}

impl SystemDnsManager for MacOsConfigurator {
    fn apply(&self, domains: &[String]) -> anyhow::Result<()> {
        std::fs::create_dir_all(RESOLVER_DIR)?;

        Self::remove_all_files()?;

        let content = Self::resolver_content();
        for domain in domains {
            std::fs::write(Self::resolver_path(&domain.replace("*.", "")), &content)?;
        }

        tracing::info!("MacOS resolver configured for {} domain(s)", domains.len());
        Ok(())
    }

    fn revert(&self) -> anyhow::Result<()> {
        Self::remove_all_files()?;
        tracing::info!("MacOS resolver configuration removed");
        Ok(())
    }
}
