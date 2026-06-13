use anyhow::Result;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if sidedns_cli::handle_daemon_start(&args) {
        return Ok(());
    }
    let rt = tokio::runtime::Runtime::new()?;

    let result = rt.block_on(async { sidedns_cli::execute_from_command_line(args).await });

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
    Ok(())
}
