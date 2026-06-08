use anyhow::Result;

pub async fn run() -> Result<()> {
    let dns_manager = sidedns_core::platform_dns_manager();
    println!("Reverting DNS configuration...");
    dns_manager.revert()?;
    println!("Cleaned up DNS configuration");
    Ok(())
}
