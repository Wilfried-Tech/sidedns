use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use tokio_rustls::rustls::{
    crypto::ring::sign,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject as _},
    sign::CertifiedKey,
};

pub const ROOT_CERT_FILENAME: &str = "SideDNS-CA.crt";
pub const ROOT_KEY_FILENAME: &str = "SideDNS-CA.key";

#[macro_export]
macro_rules! cert_path {
    () => {
        $crate::ROOT_CERTIFICATE_DIR.join($crate::certs::ca::ROOT_CERT_FILENAME)
    };
    ($dir:expr) => {
        $dir.join($crate::certs::ca::ROOT_CERT_FILENAME)
    };
}

#[macro_export]
macro_rules! cert_key_path {
    () => {
        $crate::ROOT_CERTIFICATE_DIR.join($crate::certs::ca::ROOT_KEY_FILENAME)
    };
    ($dir:expr) => {
        $dir.join($crate::certs::ca::ROOT_KEY_FILENAME)
    };
}

#[derive(Debug)]
pub struct Ca {
    issuer: Issuer<'static, KeyPair>,
}

pub struct CaPem {
    cert: String,
    key: String,
}

impl Ca {
    pub fn load_or_generate(path: PathBuf) -> Result<Self> {
        let res = Self::load(path.to_path_buf());
        if let Ok(ca) = res {
            return Ok(ca);
        }

        let (ca_pem, ca) = Self::generate()?;

        Self::save(path, ca_pem)?;

        Ok(ca)
    }

    pub fn sign(&self, domains: &[&str]) -> Result<CertifiedKey> {
        anyhow::ensure!(!domains.is_empty(), "At least one domain required");

        let domain_key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;

        let mut params =
            CertificateParams::new(domains.iter().map(ToString::to_string).collect::<Vec<_>>())?;

        domains
            .iter()
            .for_each(|d| params.distinguished_name.push(DnType::CommonName, *d));

        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];

        let cert = params.signed_by(&domain_key, &self.issuer)?;

        let cert_der = cert.der().clone().into_owned();
        let key_der =
            PrivateKeyDer::try_from(domain_key.serialize_der()).map_err(|err| anyhow!(err))?;

        let signing_key = sign::any_supported_type(&key_der)
            .map_err(|e| anyhow::anyhow!("rustls signing key error: {e}"))?;

        Ok(CertifiedKey::new(vec![cert_der], signing_key))
    }

    pub fn is_installed() -> bool {
        Self::load(crate::ROOT_CERTIFICATE_DIR.to_path_buf()).is_ok()
    }

    pub fn load(cert_root_dir: PathBuf) -> Result<Self> {
        let cert_path = cert_path!(cert_root_dir);
        let key_path = cert_key_path!(cert_root_dir);

        if cert_path.exists() && key_path.exists() {
            let ca_cert_der =
                CertificateDer::from_pem_file(cert_path).context("Failed to read CA cert")?;
            let ca_key_der =
                PrivateKeyDer::from_pem_file(key_path).context("Failed to read CA key")?;

            let ca_key =
                KeyPair::from_der_and_sign_algo(&ca_key_der, &rcgen::PKCS_ECDSA_P256_SHA256)?;
            let issuer = Issuer::from_ca_cert_der(&ca_cert_der, ca_key)?;

            return Ok(Self { issuer });
        }
        anyhow::bail!(
            "Failed to load CA from {}. Did you install it with 'sidedns cert install'?",
            cert_path.display()
        );
    }

    pub fn generate() -> Result<(CaPem, Self)> {
        let ca_key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;

        let mut params = CertificateParams::new(vec![])?;

        params
            .distinguished_name
            .push(DnType::CommonName, crate::CERT_NAME);
        params
            .distinguished_name
            .push(DnType::OrganizationName, "SideDNS");
        params.distinguished_name.push(
            DnType::OrganizationalUnitName,
            Self::get_user_and_hostname(),
        );

        params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];

        let ca_cert = params.self_signed(&ca_key)?;
        let ca_pem = ca_cert.pem();
        let key_pem = ca_key.serialize_pem();

        let issuer = Issuer::from_ca_cert_pem(&ca_pem, ca_key)?;

        Ok((
            CaPem {
                cert: ca_pem,
                key: key_pem,
            },
            Self { issuer },
        ))
    }

    fn save(cert_root_dir: PathBuf, ca_pem: CaPem) -> Result<()> {
        std::fs::create_dir_all(cert_root_dir.as_path())?;
        std::fs::write(cert_path!(cert_root_dir), &ca_pem.cert)?;
        std::fs::write(cert_key_path!(cert_root_dir), &ca_pem.key)?;
        Ok(())
    }

    pub fn uninstall(cert_root_dir: PathBuf) -> Result<()> {
        std::fs::remove_file(cert_path!(cert_root_dir))?;
        std::fs::remove_file(cert_key_path!(cert_root_dir))?;
        Ok(())
    }

    fn get_user_and_hostname() -> String {
        let user = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .map(|s| s + "@")
            .unwrap_or_default();
        let host = std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("COMPUTERNAME"))
            .unwrap_or_default();

        format!("{user}{host}")
    }
}

pub fn load_or_generate() -> Result<Ca> {
    Ca::load_or_generate(crate::ROOT_CERTIFICATE_DIR.to_path_buf())
}

pub fn load() -> Result<Ca> {
    Ca::load(crate::ROOT_CERTIFICATE_DIR.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_sign() {
        let (_, ca) = Ca::generate().unwrap();
        let cert = ca.sign(&["api.local"]).unwrap();
        assert!(!cert.cert.is_empty());
    }

    #[test]
    fn generate_sign_multiple_sans() {
        let (_, ca) = Ca::generate().unwrap();
        let cert = ca
            .sign(&["api.local", "auth.local", "*.dev.local"])
            .unwrap();
        assert!(!cert.cert.is_empty());
    }

    #[test]
    fn save_and_load_produces_valid_issuer() {
        let dir = tempfile::tempdir().unwrap();
        let (ca_pem, _) = Ca::generate().unwrap();
        Ca::save(dir.path().into(), ca_pem).unwrap();

        let loaded = Ca::load(dir.path().into()).unwrap();

        let cert = loaded.sign(&["api.local"]).unwrap();
        assert!(!cert.cert.is_empty());
    }

    #[test]
    fn load_or_generate_creates_files_when_missing() {
        let dir = tempfile::tempdir().unwrap();

        let _ = Ca::load_or_generate(dir.path().into()).unwrap();
        assert!(cert_path!(dir.path()).exists());
        assert!(cert_key_path!(dir.path()).exists());
    }

    #[test]
    fn load_or_generate_reuses_existing() {
        let dir = tempfile::tempdir().unwrap();

        let ca1 = Ca::load_or_generate(dir.path().into()).unwrap();
        let ca2 = Ca::load_or_generate(dir.path().into()).unwrap();

        assert_eq!(
            ca1.issuer.key().public_key_pem(),
            ca2.issuer.key().public_key_pem()
        );
    }

    #[test]
    fn generate_produces_non_empty_pems() {
        let (pem, _) = Ca::generate().unwrap();
        assert!(!pem.cert.is_empty());
        assert!(!pem.key.is_empty());
    }

    #[test]
    fn generate_cert_pem_has_correct_markers() {
        let (pem, _) = Ca::generate().unwrap();
        assert!(pem.cert.contains("-----BEGIN CERTIFICATE-----"));
        assert!(pem.cert.contains("-----END CERTIFICATE-----"));
    }

    #[test]
    fn generate_key_pem_has_correct_markers() {
        let (pem, _) = Ca::generate().unwrap();
        assert!(pem.key.contains("-----BEGIN"));
    }

    #[test]
    fn two_generates_produce_different_keys() {
        let (pem1, _) = Ca::generate().unwrap();
        let (pem2, _) = Ca::generate().unwrap();
        assert_ne!(pem1.key, pem2.key);
        assert_ne!(pem1.cert, pem2.cert);
    }

    #[test]
    fn sign_single_domain() {
        let (_, ca) = Ca::generate().unwrap();
        let ck = ca.sign(&["api.local"]).unwrap();
        assert!(!ck.cert.is_empty());
    }

    #[test]
    fn sign_empty_domains_errors() {
        let (_, ca) = Ca::generate().unwrap();
        assert!(ca.sign(&[]).is_err());
    }

    #[test]
    fn sign_wildcard_domain() {
        let (_, ca) = Ca::generate().unwrap();
        let ck = ca.sign(&["*.local"]).unwrap();
        assert!(!ck.cert.is_empty());
    }

    #[test]
    fn sign_multiple_sans() {
        let (_, ca) = Ca::generate().unwrap();
        let ck = ca
            .sign(&["api.local", "auth.local", "*.dev.local"])
            .unwrap();
        assert!(!ck.cert.is_empty());
    }

    #[test]
    fn sign_localhost() {
        let (_, ca) = Ca::generate().unwrap();
        let ck = ca.sign(&["localhost"]).unwrap();
        assert!(!ck.cert.is_empty());
    }

    #[test]
    fn two_signs_produce_different_certs() {
        let (_, ca) = Ca::generate().unwrap();
        let ck1 = ca.sign(&["api.local"]).unwrap();
        let ck2 = ca.sign(&["api.local"]).unwrap();
        assert_ne!(ck1.cert, ck2.cert);
    }

    #[test]
    fn save_creates_cert_and_key_files() {
        let dir = tempfile::tempdir().unwrap();
        let (pem, _) = Ca::generate().unwrap();
        Ca::save(dir.path().into(), pem).unwrap();
        assert!(cert_path!(dir.path()).exists(), "cert file must exist");
        assert!(cert_key_path!(dir.path()).exists(), "key file must exist");
    }

    #[test]
    fn load_after_save_can_sign() {
        let dir = tempfile::tempdir().unwrap();
        let (pem, _) = Ca::generate().unwrap();
        Ca::save(dir.path().into(), pem).unwrap();

        let ca = Ca::load(dir.path().into()).unwrap();
        let ck = ca.sign(&["api.local"]).unwrap();
        assert!(!ck.cert.is_empty());
    }

    #[test]
    fn load_preserves_cert_pem_content() {
        let dir = tempfile::tempdir().unwrap();
        let (original_pem, _) = Ca::generate().unwrap();
        let expected_cert = original_pem.cert.clone();
        Ca::save(dir.path().into(), original_pem).unwrap();

        let on_disk = std::fs::read_to_string(cert_path!(dir.path())).unwrap();
        assert_eq!(on_disk.trim(), expected_cert.trim());
    }

    #[test]
    fn load_nonexistent_dir_returns_error() {
        let result = Ca::load("/tmp/sidedns-definitely-does-not-exist-xyz".into());
        assert!(result.is_err());
    }

    #[test]
    fn load_or_generate_creates_files_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        Ca::load_or_generate(dir.path().into()).unwrap();
        assert!(cert_path!(dir.path()).exists());
        assert!(cert_key_path!(dir.path()).exists());
    }

    #[test]
    fn load_or_generate_reuses_without_regenerating() {
        let dir = tempfile::tempdir().unwrap();

        let (pem1, _) = Ca::generate().unwrap();
        let cert1 = pem1.cert.clone();
        Ca::save(dir.path().into(), pem1).unwrap();

        Ca::load_or_generate(dir.path().into()).unwrap();

        let cert_on_disk = std::fs::read_to_string(cert_path!(dir.path())).unwrap();
        assert_eq!(cert_on_disk.trim(), cert1.trim());
    }

    #[test]
    fn load_or_generate_can_sign_after_reload() {
        let dir = tempfile::tempdir().unwrap();
        Ca::load_or_generate(dir.path().into()).unwrap();
        let ca = Ca::load_or_generate(dir.path().into()).unwrap();
        assert!(ca.sign(&["api.local"]).is_ok());
    }
}
