use anyhow::Result;
use sidedns_core::{DnsRule, IpcClient, IpcRequest, IpcResponse};

use crate::cli::ResolveArgs;

pub async fn run(args: ResolveArgs) -> Result<()> {
    let client = IpcClient::default();

    let request = IpcRequest::Resolve {
        domain: args.domain.to_string(),
    };

    let domain = args.domain;

    match client.send(request).await? {
        IpcResponse::Resolved(Some(DnsRule {
            domain,
            target,
            https,
            ..
        })) => {
            let scheme = if https { "https" } else { "http" };
            println!(
                "Resolved: {scheme}://{domain} → {}:{}",
                target.ip(),
                target.port()
            )
        },
        IpcResponse::Resolved(None) => println!("No DNS rule for '{domain}'"),
        IpcResponse::Error(e) => anyhow::bail!(e),
        _ => {},
    }

    Ok(())
}
