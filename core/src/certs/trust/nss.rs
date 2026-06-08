use std::path::{Path, PathBuf};
use std::process::Command;

use super::TrustStore;

pub struct NssStore;

impl TrustStore for NssStore {
    fn name(&self) -> &str {
        "nss"
    }

    fn is_available(&self) -> bool {
        certutil_path().is_some()
    }

    fn is_installed(&self, _cert_path: &Path) -> bool {
        let Some(certutil) = certutil_path() else {
            return false;
        };
        nss_profiles().iter().any(|profile| {
            Command::new(&certutil)
                .args([
                    "-V",
                    "-u",
                    "L",
                    "-d",
                    profile.to_str().unwrap(),
                    "-n",
                    crate::CERT_NAME,
                ])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        })
    }

    fn install(&self, cert_path: &Path) -> anyhow::Result<()> {
        let certutil =
            certutil_path().ok_or_else(|| anyhow::anyhow!("{}", certutil_missing_hint()))?;

        let profiles = nss_profiles();
        if profiles.is_empty() {
            tracing::warn!(
                "no Firefox/Chrome profiles found — \
                 start the browser at least once, then re-run 'sidedns cert trust [--nss]'"
            );
            return Ok(());
        }

        let mut any_ok = false;
        for profile in &profiles {
            let output = Command::new(&certutil)
                .args([
                    "-A",
                    "-d",
                    profile.to_str().unwrap(),
                    "-t",
                    "C,,",
                    "-n",
                    crate::CERT_NAME,
                    "-i",
                    cert_path.to_str().unwrap(),
                ])
                .output()?;

            if output.status.success() {
                any_ok = true;
                tracing::debug!("NSS installed in {}", profile.display());
            } else {
                tracing::warn!(
                    "NSS install failed in {}: {}",
                    profile.display(),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }

        if !any_ok {
            anyhow::bail!("NSS install failed in all profiles — see warnings above");
        }
        Ok(())
    }

    fn uninstall(&self, _cert_path: &Path) -> anyhow::Result<()> {
        let Some(certutil) = certutil_path() else {
            tracing::warn!("{}", certutil_missing_hint());
            return Ok(());
        };

        for profile in nss_profiles() {
            // Check if it's installed before trying to remove
            let check = Command::new(&certutil)
                .args([
                    "-V",
                    "-u",
                    "L",
                    "-d",
                    profile.to_str().unwrap(),
                    "-n",
                    crate::CERT_NAME,
                ])
                .output()?;

            if !check.status.success() {
                continue; // not in this profile
            }

            let output = Command::new(&certutil)
                .args([
                    "-D",
                    "-d",
                    profile.to_str().unwrap(),
                    "-n",
                    crate::CERT_NAME,
                ])
                .output()?;

            if !output.status.success() {
                tracing::warn!(
                    "NSS uninstall failed in {}: {}",
                    profile.display(),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
        Ok(())
    }
}

fn certutil_path() -> Option<PathBuf> {
    which("certutil")
}

fn certutil_missing_hint() -> String {
    #[cfg(target_os = "linux")]
    return "certutil not found — install with: \
        sudo apt install libnss3-tools OR sudo dnf install nss-tools OR sudo pacman -S nss"
        .into();

    #[cfg(target_os = "macos")]
    return "certutil not found — install with: brew install nss".into();

    #[cfg(windows)]
    return "certutil not found on Windows — NSS trust store not supported".into();

    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    "certutil not found".into()
}

/// Discover all NSS database directories (Firefox + Chrome profiles).
fn nss_profiles() -> Vec<PathBuf> {
    let mut profiles = Vec::new();

    let profiles_patterns = [nss_profile_patterns(), nss_extra_dirs()].concat();

    for pattern in profiles_patterns {
        if let Ok(entries) = glob::glob(&pattern) {
            for entry in entries.flatten() {
                if entry.is_dir() && has_nss_db(&entry) {
                    profiles.push(entry.to_path_buf());
                }
            }
        }
    }

    profiles
}

fn has_nss_db(dir: &Path) -> bool {
    // Modern NSS (sql format)
    dir.join("cert9.db").exists()
    // Legacy NSS (dbm format)
    || dir.join("cert8.db").exists()
}

fn nss_profile_patterns() -> Vec<String> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let home = dirs::home_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    #[cfg(target_os = "linux")]
    return vec![
        format!("{home}/.mozilla/firefox/*.default*"),
        format!("{home}/.mozilla/firefox/*.dev-edition-default*"),
        format!("{home}/snap/firefox/common/.mozilla/firefox/*.default*"),
        format!("/var/snap/firefox/common/.mozilla/firefox/*.default*"),
    ];

    #[cfg(target_os = "macos")]
    return vec![format!(
        "{home}/Library/Application Support/Firefox/Profiles/*"
    )];

    #[cfg(windows)]
    {
        let appdata = std::env::var("APPDATA").unwrap_or_default();
        return vec![format!("{appdata}\\Mozilla\\Firefox\\Profiles\\*")];
    };

    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    vec![]
}

fn nss_extra_dirs() -> Vec<String> {
    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        [
            ".pki/nssdb",
            "snap/chromium/current/.pki/nssdb",
            ".config/google-chrome/NSSCertificateDatabase",
            ".config/chromium/NSSCertificateDatabase",
            "*/.zen",
            "*/.waterfox",
            "*/.librewolf",
            "*/.var/app",
            "*/snap",
        ]
        .iter()
        .map(|path| format!("{home}/{path}"))
        .collect()
    }

    #[cfg(not(target_os = "linux"))]
    vec![]
}

fn which(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").as_ref().and_then(|path| {
        std::env::split_paths(path).find_map(|dir| {
            let candidate = dir.join(name);
            if candidate.is_file() {
                Some(candidate)
            } else {
                #[cfg(windows)]
                {
                    let with_exe = dir.join(format!("{name}.exe"));
                    if with_exe.is_file() {
                        return Some(with_exe);
                    }
                }
                None
            }
        })
    })
}
