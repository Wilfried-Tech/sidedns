use anyhow::{Result, bail};
use sidedns_core::{IpcClient, IpcRequest, IpcResponse};

use crate::cli::RemoveArgs;

pub async fn run(args: RemoveArgs) -> Result<()> {
    let client = IpcClient::default();

    let request = IpcRequest::Remove {
        domain: args.domain.to_string(),
    };

    match client.send(request).await? {
        IpcResponse::Ok => println!("Dns rule removed for: {}", args.domain),
        IpcResponse::Error(e) => bail!(e),
        _ => {},
    }

    Ok(())
}
