use anyhow::Result;

use crate::cli::{CERT_TRUST_DEFAULT_ARGS, CertInstallArgs, CertTrustArgs};
use sidedns_core::{cert_path, certs};

pub async fn install(args: CertInstallArgs) -> Result<()> {
    anyhow::ensure!(
        is_superuser::is_superuser() && args.trust,
        "sidedns cert --trust must be run as root/administrator"
    );

    if certs::Ca::is_installed() && !args.force {
        println!("{} Already Installed", sidedns_core::CERT_NAME);
        println!("Use '--force' to re-generate and re-install the certificate.");
        return Ok(());
    }

    println!(
        "Generating new {} certificate... {}",
        sidedns_core::CERT_NAME,
        if certs::Ca::is_installed() {
            "(overwriting existing one)"
        } else {
            ""
        }
    );
    certs::ca::load_or_generate()?;

    println!(
        "{} Is now installed on your system.\nCertificate path: {}",
        sidedns_core::CERT_NAME,
        cert_path!().display()
    );

    if !args.trust {
        println!("Run 'sidedns cert trust' to trust it in supported stores.")
    } else {
        trust(CERT_TRUST_DEFAULT_ARGS.clone()).await?;
    }

    Ok(())
}

pub async fn uninstall() -> Result<()> {
    if !certs::Ca::is_installed() {
        println!(
            "{} is not installed. Nothing to uninstall.",
            sidedns_core::CERT_NAME
        );
        return Ok(());
    }

    anyhow::ensure!(
        is_superuser::is_superuser(),
        "sidedns cert uninstall must be run as root/administrator"
    );

    println!("Untrusting {} from all stores...", sidedns_core::CERT_NAME);
    untrust(CERT_TRUST_DEFAULT_ARGS.clone()).await?;

    println!("Uninstalling {}...", sidedns_core::CERT_NAME);
    certs::Ca::uninstall(sidedns_core::ROOT_CERTIFICATE_DIR.clone())?;

    println!(
        "{} has been uninstalled from your system.",
        sidedns_core::CERT_NAME
    );

    Ok(())
}

pub async fn trust(args: CertTrustArgs) -> Result<()> {
    if !certs::Ca::is_installed() {
        println!(
            "{} is not installed. Run 'sidedns cert install' first.",
            sidedns_core::CERT_NAME
        );
        return Ok(());
    }

    anyhow::ensure!(
        is_superuser::is_superuser(),
        "sidedns cert trust must be run as root/administrator"
    );

    let stores = certs::trust::available_stores();
    if stores.is_empty() {
        println!("No trust stores available to trust the certificate.");
        return Ok(());
    }

    for store in stores {
        if args.all
            || (args.system && store.name() == "system")
            || (args.java && store.name() == "java")
            || (args.nss && store.name() == "nss")
        {
            match store.install(cert_path!().as_path()) {
                Ok(_) => println!("Trusted in {} store.", store.name()),
                Err(e) => println!("Failed to trust in {} store: {e}", store.name()),
            }
        }
    }

    Ok(())
}

pub async fn untrust(args: CertTrustArgs) -> Result<()> {
    if !certs::Ca::is_installed() {
        println!(
            "{} is not installed. Run 'sidedns cert install' first.",
            sidedns_core::CERT_NAME
        );
        return Ok(());
    }

    anyhow::ensure!(
        is_superuser::is_superuser(),
        "sidedns cert untrust must be run as root/administrator"
    );

    let stores = certs::trust::available_stores();
    if stores.is_empty() {
        println!("No trust stores available to untrust the certificate.");
        return Ok(());
    }

    for store in stores {
        if args.all
            || (args.system && store.name() == "system")
            || (args.java && store.name() == "java")
            || (args.nss && store.name() == "nss")
        {
            match store.uninstall(cert_path!().as_path()) {
                Ok(_) => println!("Untrusted in {} store.", store.name()),
                Err(e) => println!("Failed to untrust in {} store: {e}", store.name()),
            }
        }
    }

    Ok(())
}
