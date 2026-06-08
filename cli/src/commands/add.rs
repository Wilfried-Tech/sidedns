use std::net::SocketAddr;

use anyhow::Result;
use sidedns_core::{IpcClient, IpcRequest, IpcResponse};

use crate::cli::AddArgs;

pub async fn run(args: AddArgs) -> Result<()> {
    let client = IpcClient::default();

    let request = IpcRequest::Add {
        domain: args.domain.to_string(),
        target: SocketAddr::new(args.ip, args.port),
        https: args.https,
    };

    match client.send(request).await? {
        IpcResponse::Ok => {
            let scheme = if args.https { "https" } else { "http" };
            println!(
                "Added: {scheme}://{} → {}:{}",
                args.domain, args.ip, args.port
            );
        },
        IpcResponse::Error(e) => anyhow::bail!(e),
        _ => {},
    }

    Ok(())
}
