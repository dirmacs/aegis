//! Certificate Authority for the aegis overlay network.
//!
//! Uses Ed25519 for signing. Each node gets a keypair + certificate signed by the CA.
//! Certificates embed the node's overlay IP, name, and groups — like Nebula's approach.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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

        tracing::info!(
            "Generated CA '{}' (expires {})",
            name,
            expires.format("%Y-%m-%d")
        );

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
            name,
            ip,
            groups,
            expires.format("%Y-%m-%d")
        );

        Ok(cert)
    }

    /// Verify a node certificate against this CA.
    pub fn verify(&self, cert: &NodeCertificate) -> Result<()> {
        self.ca_cert.verify(cert)
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

impl CaCertificate {
    /// Load a CA certificate from disk (`ca.cert`), validating its public key,
    /// fingerprint, and expiration without requiring the CA private key.
    pub fn load(path: &Path) -> Result<Self> {
        let cert_json =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let ca_cert: Self = serde_json::from_str(&cert_json)
            .with_context(|| format!("parsing {}", path.display()))?;
        ca_cert.validate()?;
        Ok(ca_cert)
    }

    /// Validate that the CA certificate itself has a well-formed Ed25519
    /// verifying key, a matching fingerprint, and has not expired.
    pub fn validate(&self) -> Result<VerifyingKey> {
        let key_bytes: [u8; 32] = self
            .public_key
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid CA public key length"))?;
        let verifying_key = VerifyingKey::from_bytes(&key_bytes)
            .map_err(|e| anyhow::anyhow!("invalid CA Ed25519 public key: {e}"))?;
        let expected_fp = CertificateAuthority::compute_fingerprint(&self.public_key);
        if self.fingerprint != expected_fp {
            bail!("CA certificate fingerprint mismatch");
        }
        if Utc::now() > self.expires_at {
            bail!("CA certificate has expired ({})", self.expires_at);
        }
        Ok(verifying_key)
    }

    /// Verify a node certificate against this CA certificate.
    pub fn verify(&self, cert: &NodeCertificate) -> Result<()> {
        let verifying_key = self.validate()?;

        if cert.ca_fingerprint != self.fingerprint {
            bail!("certificate was not signed by this CA");
        }

        if Utc::now() > cert.expires_at {
            bail!("certificate has expired ({})", cert.expires_at);
        }

        let payload = cert.signing_payload()?;
        let sig_bytes: [u8; 64] = cert
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid signature length"))?;
        let signature = Signature::from_bytes(&sig_bytes);
        verifying_key
            .verify(&payload, &signature)
            .map_err(|e| anyhow::anyhow!("signature verification failed: {e}"))?;

        Ok(())
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
        let mut file = create_owner_only_file(&path)?;
        file.write_all(json.as_bytes())?;
        Ok(path)
    }

    /// Load node cert from disk.
    pub fn load(path: &Path) -> Result<Self> {
        let json =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let cert: Self =
            serde_json::from_str(&json).with_context(|| format!("parsing {}", path.display()))?;
        Ok(cert)
    }

    /// Check if the certificate is currently valid (not expired).
    pub fn is_valid(&self) -> bool {
        Utc::now() < self.expires_at
    }

    /// Encode this node certificate as standard base64 JSON suitable for an
    /// HTTP header (`X-Aegis-Cert`).
    pub fn encode_header(&self) -> Result<String> {
        let json = serde_json::to_vec(self)?;
        Ok(base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            json,
        ))
    }

    /// Decode a node certificate from an `X-Aegis-Cert` header value (either
    /// base64-encoded JSON or raw JSON).
    pub fn decode_header(raw: &str) -> Result<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            bail!("empty node certificate header");
        }
        let bytes = if trimmed.starts_with('{') {
            trimmed.as_bytes().to_vec()
        } else {
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, trimmed)
                .context("decoding base64 node certificate")?
        };
        let cert: Self = serde_json::from_slice(&bytes).context("parsing node certificate JSON")?;
        Ok(cert)
    }

    /// Sign an HTTP request proof `(timestamp, method, path)` with a node's
    /// Ed25519 signing key, returning standard base64.
    pub fn sign_request(
        signing_key: &SigningKey,
        timestamp: i64,
        method: &str,
        path: &str,
    ) -> String {
        let payload = format!("{timestamp}\n{method}\n{path}");
        let sig = signing_key.sign(payload.as_bytes());
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, sig.to_bytes())
    }

    /// Verify an HTTP request proof signature against this certificate's
    /// Ed25519 public key.
    pub fn verify_request(
        &self,
        timestamp: i64,
        method: &str,
        path: &str,
        signature_b64: &str,
    ) -> Result<()> {
        let key_bytes: [u8; 32] = self
            .public_key
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid node public key length"))?;
        let verifying_key = VerifyingKey::from_bytes(&key_bytes)
            .map_err(|e| anyhow::anyhow!("invalid node Ed25519 public key: {e}"))?;
        let sig_vec = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            signature_b64.trim(),
        )
        .context("decoding base64 request signature")?;
        let sig_bytes: [u8; 64] = sig_vec
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid request signature length"))?;
        let signature = Signature::from_bytes(&sig_bytes);
        let payload = format!("{timestamp}\n{method}\n{path}");
        verifying_key
            .verify(payload.as_bytes(), &signature)
            .map_err(|e| anyhow::anyhow!("request signature verification failed: {e}"))?;
        Ok(())
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

/// Create (or truncate) `path` for writing with owner-only permissions (0o600).
///
/// The mode rides along with the `O_CREAT` call, so it is applied to a file this
/// function creates and to nothing else. An existing path — including a symlink
/// — is never chmod'ed, so no symlink is followed or dereferenced to set the
/// mode. A file that already exists with a wider mode keeps it (the mode
/// argument is ignored unless the file is created here); tightening such a file
/// would require `set_permissions`, which resolves symlinks, so it is
/// deliberately left to the owner of the path.
#[cfg(unix)]
fn create_owner_only_file(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn create_owner_only_file(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ca_generate_and_sign() {
        let ca = CertificateAuthority::generate("test-ca", 365).unwrap();

        // Only the public key is needed here to sign the cert; the node's
        // private signing key is not exercised by this issuance test.
        let (_node_key, node_pub) = NodeKeypair::generate();
        let cert = ca
            .sign_node(
                "test-node",
                "10.42.0.1",
                &["servers".to_string()],
                &node_pub.to_bytes(),
                365,
            )
            .unwrap();

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
        let cert = ca
            .sign_node(
                "node1",
                "10.42.0.1",
                &["servers".to_string()],
                &node_pub.to_bytes(),
                365,
            )
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        cert.save(dir.path()).unwrap();

        let loaded = NodeCertificate::load(&dir.path().join("node1.cert")).unwrap();
        assert_eq!(cert.name, loaded.name);
        assert_eq!(cert.ip, loaded.ip);
        ca.verify(&loaded).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn node_cert_save_is_owner_only() {
        use std::os::unix::fs::MetadataExt;

        let ca = CertificateAuthority::generate("test-ca", 365).unwrap();
        let (_node_key, node_pub) = NodeKeypair::generate();
        let cert = ca
            .sign_node(
                "node1",
                "10.42.0.1",
                &["servers".to_string()],
                &node_pub.to_bytes(),
                365,
            )
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        cert.save(dir.path()).unwrap();

        let mode = std::fs::metadata(dir.path().join("node1.cert"))
            .unwrap()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "cert file mode was {mode:o}");
    }

    #[cfg(unix)]
    #[test]
    fn node_cert_save_does_not_change_mode_through_symlink() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let ca = CertificateAuthority::generate("test-ca", 365).unwrap();
        let (_node_key, node_pub) = NodeKeypair::generate();
        let cert = ca
            .sign_node(
                "node1",
                "10.42.0.1",
                &["servers".to_string()],
                &node_pub.to_bytes(),
                365,
            )
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.cert");
        let link = dir.path().join("node1.cert");
        std::fs::write(&target, "existing").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        cert.save(dir.path()).unwrap();

        let mode = std::fs::symlink_metadata(&target).unwrap().mode() & 0o777;
        assert_eq!(mode, 0o644, "symlink target mode was {mode:o}");
    }
}
