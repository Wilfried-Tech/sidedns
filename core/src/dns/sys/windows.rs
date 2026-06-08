use super::SystemDnsManager;

const COMMENT: &str = "Managed by SideDNS — do not edit manually";

/// Configures split DNS via the Windows Name Resolution Policy Table (NRPT).
///
/// Uses PowerShell's `Add-DnsClientNrptRule` and `Remove-DnsClientNrptRule`.
/// Rules are tagged with a comment so they can be identified and removed cleanly.
///
/// Requires administrator privileges.
pub struct WindowsNrptConfigurator;

impl WindowsNrptConfigurator {
    fn powershell(script: &str) -> anyhow::Result<()> {
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .status()?;
        anyhow::ensure!(status.success(), "PowerShell command failed: {script}");
        Ok(())
    }

    fn remove_all_rules() -> anyhow::Result<()> {
        let script = format!(
            "Get-DnsClientNrptRule | Where-Object {{ $_.Comment -eq '{COMMENT}' }} | \
             Remove-DnsClientNrptRule -Force"
        );
        Self::powershell(&script)
    }
}

impl SystemDnsManager for WindowsNrptConfigurator {
    fn apply(&self, domains: &[String]) -> anyhow::Result<()> {
        Self::remove_all_rules()?;
        let dns_server = super::DNS_LISTEN_ADDR.ip();
        let domains = domains
            .iter()
            .map(|d| {
                if d.starts_with("*.") {
                    format!(".{}", d.strip_prefix("*.").unwrap_or(d))
                } else {
                    d.to_string()
                }
            })
            .collect::<Vec<_>>();
        for domain in &domains {
            let script = format!(
                "Add-DnsClientNrptRule \
                 -Namespace '{domain}' \
                 -NameServers '{dns_server}' \
                 -Comment '{COMMENT}'"
            );
            Self::powershell(&script)?;
        }

        tracing::info!("Windows NRPT configured for {} domain(s)", domains.len());
        Ok(())
    }

    fn revert(&self) -> anyhow::Result<()> {
        Self::remove_all_rules()?;
        tracing::info!("Windows NRPT rules removed");
        Ok(())
    }
}
