pub mod java;
pub mod nss;
pub mod system;

use std::path::Path;

/// A trust store that can install and uninstall a CA certificate.
pub trait TrustStore: Send + Sync {
    /// Human-readable name shown in log output.
    fn name(&self) -> &str;

    /// Returns `true` if the required tools are present on this machine.
    fn is_available(&self) -> bool;

    /// Returns `true` if the CA at `cert_path` is already trusted.
    fn is_installed(&self, cert_path: &Path) -> bool;

    /// Install the CA certificate into this trust store.
    fn install(&self, cert_path: &Path) -> anyhow::Result<()>;

    /// Remove the CA certificate from this trust store.
    fn uninstall(&self, cert_path: &Path) -> anyhow::Result<()>;
}

/// Return all trust store implementations available.
pub fn available_stores() -> Vec<Box<dyn TrustStore>> {
    vec![
        Box::new(system::SystemStore),
        Box::new(nss::NssStore),
        Box::new(java::JavaStore),
    ]
}
