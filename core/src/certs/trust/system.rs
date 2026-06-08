use std::path::Path;

use tokio_rustls::rustls::pki_types::{CertificateDer, pem::PemObject};

use super::TrustStore;

pub struct SystemStore;

impl TrustStore for SystemStore {
    fn name(&self) -> &str {
        "system"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn is_installed(&self, cert_path: &Path) -> bool {
        is_installed_impl(cert_path)
    }

    fn install(&self, cert_path: &Path) -> anyhow::Result<()> {
        install_impl(cert_path)
    }

    fn uninstall(&self, cert_path: &Path) -> anyhow::Result<()> {
        uninstall_impl(cert_path)
    }
}

// ─── macOS ────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn is_installed_impl(cert_path: &Path) -> bool {
    let fingerprint = match sha1_fingerprint(cert_path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let output = std::process::Command::new("security")
        .args([
            "find-certificate",
            "-a",
            "-Z",
            "/Library/Keychains/System.keychain",
        ])
        .output();
    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).contains(&fingerprint),
        Err(_) => false,
    }
}

#[cfg(target_os = "macos")]
fn install_impl(cert_path: &Path) -> anyhow::Result<()> {
    let status = std::process::Command::new("sudo")
        .args([
            "security",
            "add-trusted-cert",
            "-d",
            "-r",
            "trustRoot",
            "-k",
            "/Library/Keychains/System.keychain",
            cert_path.to_str().unwrap(),
        ])
        .status()?;
    anyhow::ensure!(status.success(), "security add-trusted-cert failed");
    Ok(())
}

#[cfg(target_os = "macos")]
fn uninstall_impl(cert_path: &Path) -> anyhow::Result<()> {
    let status = std::process::Command::new("sudo")
        .args([
            "security",
            "remove-trusted-cert",
            "-d",
            cert_path.to_str().unwrap(),
        ])
        .status()?;
    anyhow::ensure!(status.success(), "security remove-trusted-cert failed");
    Ok(())
}

// ─── Linux ────────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn is_installed_impl(cert_path: &Path) -> bool {
    // Check if our CA is trusted by the system (probe via openssl if available)
    let success = std::process::Command::new("openssl")
        .args([
            "verify",
            "-CAfile",
            "/etc/ssl/certs/ca-certificates.crt",
            cert_path.to_str().unwrap(),
        ])
        .output()
        .ok()
        .map(|o| o.status.success())
        .unwrap_or(false);
    // Rough heuristic: check if the sidedns cert file exists in the right place
    success
        || std::path::Path::new("/usr/local/share/ca-certificates/sidedns.crt").exists()
        || std::path::Path::new("/etc/pki/ca-trust/source/anchors/sidedns.crt").exists()
        || std::path::Path::new("/etc/ca-certificates/trust-source/sidedns.pem").exists()
}

#[cfg(target_os = "linux")]
fn install_impl(cert_path: &Path) -> anyhow::Result<()> {
    // Try each supported distribution in order
    let debian = std::path::Path::new("/usr/local/share/ca-certificates");
    let fedora = std::path::Path::new("/etc/pki/ca-trust/source/anchors");
    let arch = std::path::Path::new("/etc/ca-certificates/trust-source");

    if debian.exists() {
        std::fs::copy(cert_path, debian.join("sidedns.crt"))?;
        let s = std::process::Command::new("sudo")
            .arg("update-ca-certificates")
            .status()?;
        anyhow::ensure!(s.success(), "update-ca-certificates failed");
    } else if fedora.exists() {
        std::fs::copy(cert_path, fedora.join("sidedns.crt"))?;
        let s = std::process::Command::new("sudo")
            .args(["update-ca-trust", "extract"])
            .status()?;
        anyhow::ensure!(s.success(), "update-ca-trust extract failed");
    } else if arch.exists() {
        std::fs::copy(cert_path, arch.join("anchors/sidedns.pem"))?;
        let s = std::process::Command::new("sudo")
            .args(["trust", "anchor", "--store", cert_path.to_str().unwrap()])
            .status()?;
        anyhow::ensure!(s.success(), "trust anchor --store failed");
    } else {
        anyhow::bail!(
            "unsupported Linux distribution — install manually:\n\
             copy {:?} to your system CA directory and run the appropriate update command",
            cert_path
        );
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn uninstall_impl(_cert_path: &Path) -> anyhow::Result<()> {
    let debian_dest = std::path::Path::new("/usr/local/share/ca-certificates/sidedns.crt");
    let fedora_dest = std::path::Path::new("/etc/pki/ca-trust/source/anchors/sidedns.crt");
    let arch_dest = std::path::Path::new("/etc/ca-certificates/trust-source/anchors/sidedns.pem");

    if debian_dest.exists() {
        std::fs::remove_file(debian_dest)?;
        std::process::Command::new("sudo")
            .arg("update-ca-certificates")
            .status()?;
    } else if fedora_dest.exists() {
        std::fs::remove_file(fedora_dest)?;
        std::process::Command::new("sudo")
            .args(["update-ca-trust", "extract"])
            .status()?;
    } else if arch_dest.exists() {
        std::fs::remove_file(arch_dest)?;
        std::process::Command::new("sudo")
            .args(["trust", "anchor", "--remove", arch_dest.to_str().unwrap()])
            .status()?;
    }

    Ok(())
}

// ─── Windows ──────────────────────────────────────────────────────────────────

#[cfg(windows)]
fn is_installed_impl(_cert_path: &Path) -> bool {
    let output = std::process::Command::new("certutil")
        .args(["-store", "Root"])
        .output();
    match output {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            text.contains("SideDNS")
        },
        Err(_) => false,
    }
}

#[cfg(windows)]
fn install_impl(cert_path: &Path) -> anyhow::Result<()> {
    let status = std::process::Command::new("certutil.exe")
        .args(["-addstore", "-f", "ROOT", cert_path.to_str().unwrap()])
        .status()?;
    anyhow::ensure!(status.success(), "certutil -addstore failed");
    Ok(())
}

#[cfg(windows)]
fn uninstall_impl(cert_path: &Path) -> anyhow::Result<()> {
    // Find by thumbprint then delete
    let fingerprint = sha1_fingerprint(cert_path)?;
    let status = std::process::Command::new("certutil")
        .args(["-delstore", "ROOT", &fingerprint])
        .status()?;
    anyhow::ensure!(status.success(), "certutil -delstore failed");
    Ok(())
}

// ─── Fallback ─────────────────────────────────────────────────────────────────

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn is_installed_impl(_cert_path: &Path) -> bool {
    false
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn install_impl(cert_path: &Path) -> anyhow::Result<()> {
    anyhow::bail!("system trust store installation not supported on this platform")
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn uninstall_impl(_cert_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

#[allow(dead_code)]
fn sha1_fingerprint(cert_path: &Path) -> anyhow::Result<String> {
    use sha1::{Digest, Sha1};
    let data = CertificateDer::from_pem_file(cert_path)?;
    let mut hash = Sha1::new();
    hash.update(data);
    let digest = hash.finalize();
    Ok(digest
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<String>())
}
