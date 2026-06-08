use std::path::PathBuf;

use super::SystemDnsManager;

const DROP_IN_DIR: &str = "/etc/systemd/resolved.conf.d";
const DROP_IN_FILE: &str = "sidedns.conf";

/// Configures split DNS via a systemd-resolved drop-in file.
///
/// Creates `/etc/systemd/resolved.conf.d/sidedns.conf` with the current
/// set of domains. Removes the file on revert and reloads the resolver.
///
/// Requires write access to `/etc/systemd/resolved.conf.d/` (root).
pub struct SystemdConfigurator;

impl SystemdConfigurator {
    /// Returns `true` if systemd-resolved is the active resolver.
    pub fn is_available() -> bool {
        std::path::Path::new("/run/systemd/resolve/resolv.conf").exists()
            || std::path::Path::new("/etc/systemd/resolved.conf").exists()
    }

    fn drop_in_path() -> PathBuf {
        PathBuf::from(DROP_IN_DIR).join(DROP_IN_FILE)
    }

    fn reload() -> anyhow::Result<()> {
        let status = std::process::Command::new("systemctl")
            .args(["reload-or-restart", "systemd-resolved"])
            .status()?;
        anyhow::ensure!(status.success(), "Failed to reload systemd-resolved");
        Ok(())
    }
}

impl SystemDnsManager for SystemdConfigurator {
    fn apply(&self, domains: &[String]) -> anyhow::Result<()> {
        std::fs::create_dir_all(DROP_IN_DIR)?;

        let domain_list: String = domains
            .iter()
            .map(|d| {
                if d.starts_with("*.") {
                    format!("~{}", d.strip_prefix("*.").unwrap_or(d))
                } else {
                    format!("~{}.", d)
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let dns_ip = super::DNS_LISTEN_ADDR.ip();
        let content = format!(
            "# Managed by SideDNS — do not edit manually\n\
             [Resolve]\n\
             DNS={dns_ip}\n\
             Domains={domain_list}\n"
        );

        std::fs::write(Self::drop_in_path(), content)?;
        Self::reload()?;

        tracing::info!(
            "systemd-resolved configured for {} domain(s)",
            domains.len()
        );
        Ok(())
    }

    fn revert(&self) -> anyhow::Result<()> {
        let path = Self::drop_in_path();
        if path.exists() {
            std::fs::remove_file(&path)?;
            tracing::info!("systemd-resolved configuration removed");
        }
        Self::reload()?;
        Ok(())
    }
}
