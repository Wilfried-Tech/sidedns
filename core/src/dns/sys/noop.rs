use super::SystemDnsManager;

/// A no-op DNS configurator that does nothing.
///
/// Used as a fallback on unsupported platforms or in tests where system
/// DNS modification is undesirable.
pub struct NoOpConfigurator;

impl SystemDnsManager for NoOpConfigurator {
    fn apply(&self, _domains: &[String]) -> anyhow::Result<()> {
        tracing::warn!("NoOpConfigurator in use — DNS system config disabled");
        Ok(())
    }

    fn revert(&self) -> anyhow::Result<()> {
        tracing::warn!("NoOpConfigurator in use — DNS system config disabled");
        Ok(())
    }
}
