//! Certificate Authority for the aegis overlay network.
//!
//! Uses Ed25519 for signing. Each node gets a keypair + certificate signed by the CA.
//! Certificates embed the node's overlay IP, name, and groups — like Nebula's approach.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{SigningKey, VerifyingKey, Signer, Signature, Verifier};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};

/// A certificate authority.
#[derive(Debug)]
pub struct CertificateAuthority {
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
    pub ca_cert: CaCertificate,
}

/// The CA's self-signed certificate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaCertificate {
    pub name: String,
    pub public_key: Vec<u8>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub fingerprint: String,
}

/// A signed node certificate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCertificate {
    pub name: String,
    pub ip: String,
    pub groups: Vec<String>,
    pub public_key: Vec<u8>,
    pub ca_fingerprint: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub signature: Vec<u8>,
}

/// A node's keypair (private key + certificate).
#[derive(Debug)]
pub struct NodeKeypair {
    pub signing_key: SigningKey,
    pub certificate: NodeCertificate,
}

impl CertificateAuthority {
    /// Create a new CA with a fresh Ed25519 keypair.
    pub fn generate(name: &str, duration_days: u32) -> Result<Self> {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let now = Utc::now();
        let expires = now + Duration::days(duration_days as i64);

        let public_bytes = verifying_key.to_bytes().to_vec();
        let fingerprint = Self::compute_fingerprint(&public_bytes);

        let ca_cert = CaCertificate {
            name: name.to_string(),
            public_key: public_bytes,
            created_at: now,
            expires_at: expires,
            fingerprint,
        };

        tracing::info!("Generated CA '{}' (expires {})", name, expires.format("%Y-%m-%d"));

        Ok(Self {
            signing_key,
            verifying_key,
            ca_cert,
        })
    }

    /// Sign a node certificate.
    pub fn sign_node(
        &self,
        name: &str,
        ip: &str,
        groups: &[String],
        node_public_key: &[u8],
        duration_days: u32,
    ) -> Result<NodeCertificate> {
        let now = Utc::now();

        // Node cert can't outlive the CA
        let ca_remaining = (self.ca_cert.expires_at - now).num_days();
        let actual_days = (duration_days as i64).min(ca_remaining);
        if actual_days <= 0 {
            bail!("CA certificate has expired");
        }
        let expires = now + Duration::days(actual_days);

        let mut cert = NodeCertificate {
            name: name.to_string(),
            ip: ip.to_string(),
            groups: groups.to_vec(),
            public_key: node_public_key.to_vec(),
            ca_fingerprint: self.ca_cert.fingerprint.clone(),
            created_at: now,
            expires_at: expires,
            signature: vec![],
        };

        // Sign the cert contents (everything except the signature field)
        let sign_payload = cert.signing_payload()?;
        let signature = self.signing_key.sign(&sign_payload);
        cert.signature = signature.to_bytes().to_vec();

        tracing::info!(
            "Signed cert for '{}' ({}) groups={:?} expires={}",
            name, ip, groups, expires.format("%Y-%m-%d")
        );

        Ok(cert)
    }

    /// Verify a node certificate against this CA.
    pub fn verify(&self, cert: &NodeCertificate) -> Result<()> {
        if cert.ca_fingerprint != self.ca_cert.fingerprint {
            bail!("certificate was not signed by this CA");
        }

        if Utc::now() > cert.expires_at {
            bail!("certificate has expired ({})", cert.expires_at);
        }

        let payload = cert.signing_payload()?;
        let sig_bytes: [u8; 64] = cert.signature.clone().try_into()
            .map_err(|_| anyhow::anyhow!("invalid signature length"))?;
        let signature = Signature::from_bytes(&sig_bytes);
        self.verifying_key.verify(&payload, &signature)
            .map_err(|e| anyhow::anyhow!("signature verification failed: {e}"))?;

        Ok(())
    }

    /// Save CA to disk (key + cert as separate files).
    pub fn save(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("creating CA dir {}", dir.display()))?;

        let key_path = dir.join("ca.key");
        let key_bytes = self.signing_key.to_bytes();
        std::fs::write(&key_path, key_bytes)
            .with_context(|| format!("writing {}", key_path.display()))?;

        // Restrict permissions on CA key
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))?;
        }

        let cert_path = dir.join("ca.cert");
        let cert_json = serde_json::to_string_pretty(&self.ca_cert)?;
        std::fs::write(&cert_path, cert_json)
            .with_context(|| format!("writing {}", cert_path.display()))?;

        tracing::info!("CA saved to {}", dir.display());
        Ok(())
    }

    /// Load CA from disk.
    pub fn load(dir: &Path) -> Result<Self> {
        let key_path = dir.join("ca.key");
        let key_bytes: [u8; 32] = std::fs::read(&key_path)
            .with_context(|| format!("reading {}", key_path.display()))?
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid CA key length"))?;
        let signing_key = SigningKey::from_bytes(&key_bytes);
        let verifying_key = signing_key.verifying_key();

        let cert_path = dir.join("ca.cert");
        let cert_json = std::fs::read_to_string(&cert_path)
            .with_context(|| format!("reading {}", cert_path.display()))?;
        let ca_cert: CaCertificate = serde_json::from_str(&cert_json)
            .with_context(|| format!("parsing {}", cert_path.display()))?;

        Ok(Self {
            signing_key,
            verifying_key,
            ca_cert,
        })
    }

    fn compute_fingerprint(public_key: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(public_key);
        let result = hasher.finalize();
        hex::encode(&result[..16]) // First 128 bits
    }
}

impl NodeCertificate {
    /// Compute the payload that gets signed (deterministic serialization).
    pub fn signing_payload(&self) -> Result<Vec<u8>> {
        let payload = serde_json::json!({
            "name": self.name,
            "ip": self.ip,
            "groups": self.groups,
            "public_key": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &self.public_key),
            "ca_fingerprint": self.ca_fingerprint,
            "created_at": self.created_at.to_rfc3339(),
            "expires_at": self.expires_at.to_rfc3339(),
        });
        Ok(serde_json::to_vec(&payload)?)
    }

    /// Save node cert to disk.
    pub fn save(&self, dir: &Path) -> Result<PathBuf> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{}.cert", self.name));
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json)?;
        Ok(path)
    }

    /// Load node cert from disk.
    pub fn load(path: &Path) -> Result<Self> {
        let json = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let cert: Self = serde_json::from_str(&json)
            .with_context(|| format!("parsing {}", path.display()))?;
        Ok(cert)
    }

    /// Check if the certificate is currently valid (not expired).
    pub fn is_valid(&self) -> bool {
        Utc::now() < self.expires_at
    }
}

impl NodeKeypair {
    /// Generate a new keypair for a node.
    pub fn generate() -> (SigningKey, VerifyingKey) {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        (signing_key, verifying_key)
    }

    /// Save the node's private key to disk.
    pub fn save_key(signing_key: &SigningKey, dir: &Path, name: &str) -> Result<PathBuf> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{}.key", name));
        std::fs::write(&path, signing_key.to_bytes())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }

        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ca_generate_and_sign() {
        let ca = CertificateAuthority::generate("test-ca", 365).unwrap();

        let (node_key, node_pub) = NodeKeypair::generate();
        let cert = ca.sign_node(
            "test-node",
            "10.42.0.1",
            &["servers".to_string()],
            &node_pub.to_bytes(),
            365,
        ).unwrap();

        assert_eq!(cert.name, "test-node");
        assert_eq!(cert.ip, "10.42.0.1");
        assert_eq!(cert.groups, vec!["servers"]);
        assert!(cert.is_valid());

        // Verify signature
        ca.verify(&cert).unwrap();

        // Tampered cert should fail
        let mut tampered = cert.clone();
        tampered.ip = "10.42.0.99".to_string();
        assert!(ca.verify(&tampered).is_err());
    }

    #[test]
    fn ca_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let ca = CertificateAuthority::generate("roundtrip-ca", 365).unwrap();
        ca.save(dir.path()).unwrap();

        let loaded = CertificateAuthority::load(dir.path()).unwrap();
        assert_eq!(ca.ca_cert.fingerprint, loaded.ca_cert.fingerprint);
        assert_eq!(ca.ca_cert.name, loaded.ca_cert.name);
    }

    #[test]
    fn node_cert_save_load() {
        let ca = CertificateAuthority::generate("test-ca", 365).unwrap();
        let (_node_key, node_pub) = NodeKeypair::generate();
        let cert = ca.sign_node(
            "node1",
            "10.42.0.1",
            &["servers".to_string()],
            &node_pub.to_bytes(),
            365,
        ).unwrap();

        let dir = tempfile::tempdir().unwrap();
        cert.save(dir.path()).unwrap();

        let loaded = NodeCertificate::load(&dir.path().join("node1.cert")).unwrap();
        assert_eq!(cert.name, loaded.name);
        assert_eq!(cert.ip, loaded.ip);
        ca.verify(&loaded).unwrap();
    }
}
