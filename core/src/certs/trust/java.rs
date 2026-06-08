use std::path::{Path, PathBuf};
use std::process::Command;

use super::TrustStore;

const ALIAS: &str = "sidedns-local-ca";
const STORE_PASS: &str = "changeit";

pub struct JavaStore;

impl TrustStore for JavaStore {
    fn name(&self) -> &str {
        "java"
    }

    fn is_available(&self) -> bool {
        keytool_path().is_some() && cacerts_path().is_some()
    }

    fn is_installed(&self, _cert_path: &Path) -> bool {
        let (Some(keytool), Some(cacerts)) = (keytool_path(), cacerts_path()) else {
            return false;
        };
        let output = Command::new(keytool)
            .args([
                "-list",
                "-keystore",
                cacerts.to_str().unwrap(),
                "-storepass",
                STORE_PASS,
                "-alias",
                ALIAS,
            ])
            .output();
        match output {
            Ok(o) => o.status.success(),
            Err(_) => false,
        }
    }

    fn install(&self, cert_path: &Path) -> anyhow::Result<()> {
        let keytool = keytool_path()
            .ok_or_else(|| anyhow::anyhow!("keytool not found — is a JDK installed?"))?;
        let cacerts =
            cacerts_path().ok_or_else(|| anyhow::anyhow!("Java cacerts file not found"))?;

        let output = Command::new(keytool)
            .args([
                "-importcert",
                "-noprompt",
                "-trustcacerts",
                "-alias",
                ALIAS,
                "-keystore",
                cacerts.to_str().unwrap(),
                "-storepass",
                STORE_PASS,
                "-file",
                cert_path.to_str().unwrap(),
            ])
            .output()?;

        anyhow::ensure!(
            output.status.success(),
            "keytool -importcert failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }

    fn uninstall(&self, _cert_path: &Path) -> anyhow::Result<()> {
        let (Some(keytool), Some(cacerts)) = (keytool_path(), cacerts_path()) else {
            return Ok(());
        };

        if !self.is_installed(_cert_path) {
            return Ok(());
        }

        let output = Command::new(keytool)
            .args([
                "-delete",
                "-alias",
                ALIAS,
                "-keystore",
                cacerts.to_str().unwrap(),
                "-storepass",
                STORE_PASS,
            ])
            .output()?;

        anyhow::ensure!(
            output.status.success(),
            "keytool -delete failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }
}

fn keytool_path() -> Option<PathBuf> {
    // Check JAVA_HOME first, then PATH
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let candidate = PathBuf::from(java_home).join("bin").join("keytool");
        if candidate.exists() {
            return Some(candidate);
        }
    }

    std::env::var_os("PATH").as_ref().and_then(|path| {
        std::env::split_paths(path).find_map(|dir| {
            let c = dir.join("keytool");
            if c.exists() {
                return Some(c);
            }
            #[cfg(windows)]
            {
                let c = dir.join("keytool.exe");
                if c.exists() {
                    return Some(c);
                }
            }
            None
        })
    })
}

fn cacerts_path() -> Option<PathBuf> {
    // Try JAVA_HOME
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        for relative in &["lib/security/cacerts", "jre/lib/security/cacerts"] {
            let p = PathBuf::from(&java_home).join(relative);
            if p.exists() {
                return Some(p);
            }
        }
    }

    None
}
