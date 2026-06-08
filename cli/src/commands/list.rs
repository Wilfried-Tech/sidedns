use anyhow::Result;
use sidedns_core::{IpcClient, IpcRequest, IpcResponse};

pub async fn run() -> Result<()> {
    let client = IpcClient::default();

    match client.send(IpcRequest::List).await? {
        IpcResponse::Rules(rules) if rules.is_empty() => println!("no rules configured"),
        IpcResponse::Rules(rules) => {
            let longest = rules
                .iter()
                .map(|r| r.domain.len())
                .max()
                .unwrap_or(0)
                .max(6);

            let ip_w = 23;
            let port_w = 5;
            let secure_w = 6;

            println!(
                "╔═{:═^longest$}═╦═{:═^ip_w$}═╦═{:═^port_w$}═╦═{:═^secure_w$}═╗",
                "",
                "",
                "",
                "",
                longest = longest,
                ip_w = ip_w,
                port_w = port_w,
                secure_w = secure_w
            );
            println!(
                "║ {:<longest$} ║ {:<ip_w$} ║ {:<port_w$} ║ {:<secure_w$} ║",
                "DOMAIN",
                "IP",
                "PORT",
                "SECURE",
                longest = longest,
                ip_w = ip_w,
                port_w = port_w,
                secure_w = secure_w
            );
            println!(
                "╠═{:═^longest$}═╬═{:═^ip_w$}═╬═{:═^port_w$}═╬═{:═^secure_w$}═╣",
                "",
                "",
                "",
                "",
                longest = longest,
                ip_w = ip_w,
                port_w = port_w,
                secure_w = secure_w
            );
            for r in rules {
                println!(
                    "║ {:<longest$} ║ {:<ip_w$} ║ {:<port_w$} ║ {:<secure_w$} ║",
                    r.domain,
                    r.target.ip(),
                    r.target.port(),
                    if r.https { "yes" } else { "no" },
                    longest = longest,
                    ip_w = ip_w,
                    port_w = port_w,
                    secure_w = secure_w
                );
            }
            println!(
                "╚═{:═^longest$}═╩═{:═^ip_w$}═╩═{:═^port_w$}═╩═{:═^secure_w$}═╝",
                "",
                "",
                "",
                "",
                longest = longest,
                ip_w = ip_w,
                port_w = port_w,
                secure_w = secure_w
            );
        },
        IpcResponse::Error(e) => eprintln!("error: {e}"),
        _ => {},
    }

    Ok(())
}
