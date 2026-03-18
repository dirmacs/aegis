use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::crypto::{decrypt, encrypt, derive_key, generate_salt};

/// An encrypted entry in the secret store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedEntry {
    pub encrypted_value: String,
    pub created_at: String,
    pub updated_at: String,
    pub tags: Vec<String>,
}

impl EncryptedEntry {
    fn new(encrypted_value: String, tags: Vec<String>) -> Self {
        let now = current_timestamp();
        Self {
            encrypted_value,
            created_at: now.clone(),
            updated_at: now,
            tags,
        }
    }
}

/// The main secret store that manages encrypted secrets
#[derive(Debug, Clone)]
pub struct SecretStore {
    path: PathBuf,
    entries: HashMap<String, EncryptedEntry>,
    salt: [u8; 16],
    version: String,
    encryption_key: Option<[u8; 32]>,  // Cached derived key from master password
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VaultFile {
    meta: Meta,
    secrets: HashMap<String, EncryptedEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Meta {
    salt: String,        // base64 encoded
    version: String,
}

impl SecretStore {
    /// Default vault path: ~/.config/aegis/vault.toml
    pub fn default_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow!("Could not determine config directory"))?;
        Ok(config_dir.join("aegis").join("vault.toml"))
    }

    /// Open or create a vault at the given path with the master password
    pub fn open(path: &Path, master_password: &str) -> Result<Self> {
        if path.exists() {
            Self::load_existing(path, master_password)
        } else {
            Self::create_new(path, master_password)
        }
    }

    /// Create a new vault
    fn create_new(path: &Path, master_password: &str) -> Result<Self> {
        debug!("Creating new vault at {:?}", path);

        // Generate random salt
        let salt = generate_salt();

        // Derive encryption key from master password
        let encryption_key = derive_key(master_password, &salt)?;

        let store = Self {
            path: path.to_path_buf(),
            entries: HashMap::new(),
            salt,
            version: "1.0".to_string(),
            encryption_key: Some(encryption_key),
        };

        store.save()?;
        info!("New vault created successfully");
        Ok(store)
    }

    /// Load an existing vault
    fn load_existing(path: &Path, master_password: &str) -> Result<Self> {
        debug!("Loading vault from {:?}", path);

        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read vault file at {}", path.display()))?;

        let vault_file: VaultFile = toml::from_str(&content)
            .context("Failed to parse vault file as TOML")?;

        // Decode salt from base64
        let salt_bytes = STANDARD
            .decode(vault_file.meta.salt)
            .context("Failed to decode salt from base64")?;

        if salt_bytes.len() != 16 {
            return Err(anyhow!("Invalid salt length"));
        }

        let mut salt = [0u8; 16];
        salt.copy_from_slice(&salt_bytes);

        // Derive encryption key from master password
        let encryption_key = derive_key(master_password, &salt)?;

        Ok(Self {
            path: path.to_path_buf(),
            entries: vault_file.secrets,
            salt,
            version: vault_file.meta.version,
            encryption_key: Some(encryption_key),
        })
    }

    /// Save the vault to disk
    pub fn save(&self) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory at {}", parent.display()))?;
        }

        // Encode salt as base64
        let salt_b64 = STANDARD.encode(self.salt);

        let vault_file = VaultFile {
            meta: Meta {
                salt: salt_b64,
                version: self.version.clone(),
            },
            secrets: self.entries.clone(),
        };

        let content = toml::to_string_pretty(&vault_file)
            .context("Failed to serialize vault to TOML")?;

        std::fs::write(&self.path, content)
            .with_context(|| format!("Failed to write vault file to {}", self.path.display()))?;

        debug!("Vault saved to {:?}", self.path);
        Ok(())
    }

    /// Set a secret value (encrypts and stores)
    pub fn set(&mut self, key: &str, value: &str, tags: &[&str]) -> Result<()> {
        let encryption_key = self.encryption_key
            .ok_or_else(|| anyhow!("Vault not initialized with a master password"))?;

        let encrypted_value = encrypt(value, &encryption_key)?;

        let now = current_timestamp();
        let entry = match self.entries.get_mut(key) {
            Some(entry) => {
                entry.encrypted_value = encrypted_value;
                entry.updated_at = now.clone();
                entry.tags = tags.iter().map(|&s| s.to_string()).collect();
                entry
            }
            None => {
                let entry = EncryptedEntry::new(encrypted_value, tags.iter().map(|&s| s.to_string()).collect());
                self.entries.insert(key.to_string(), entry);
                self.entries.get_mut(key).unwrap()
            }
        };

        debug!("Set secret '{}' with tags {:?}", key, entry.tags);
        Ok(())
    }

    /// Get a secret value (decrypts and returns)
    pub fn get(&self, key: &str) -> Result<String> {
        let entry = self.entries.get(key)
            .ok_or_else(|| anyhow!("Secret '{}' not found", key))?;

        let encryption_key = self.encryption_key
            .ok_or_else(|| anyhow!("Vault not initialized with a master password"))?;

        let plaintext = decrypt(&entry.encrypted_value, &encryption_key)?;

        debug!("Retrieved secret '{}'", key);
        Ok(plaintext)
    }

    /// List all secret keys and their tags (not values)
    pub fn list(&self) -> Vec<(&str, Vec<&str>)> {
        self.entries.iter()
            .map(|(k, v)| (k.as_str(), v.tags.iter().map(|s| s.as_str()).collect()))
            .collect()
    }

    /// Remove a secret
    pub fn remove(&mut self, key: &str) -> Result<()> {
        if self.entries.remove(key).is_some() {
            debug!("Removed secret '{}'", key);
            Ok(())
        } else {
            Err(anyhow!("Secret '{}' not found", key))
        }
    }

    /// Check if a secret exists
    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// Get number of secrets
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if store is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn current_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_create_and_load_vault() {
        let temp_dir = tempdir().unwrap();
        let vault_path = temp_dir.path().join("vault.toml");
        let master_password = "test_password";

        // Create new vault
        let mut store = SecretStore::open(&vault_path, master_password).unwrap();
        assert!(store.is_empty());

        // Set a secret
        store.set("api_key", "secret123", &["production"]).unwrap();
        assert_eq!(store.len(), 1);

        // Save and reload
        store.save().unwrap();
        let store2 = SecretStore::open(&vault_path, master_password).unwrap();
        assert_eq!(store2.len(), 1);
        assert!(store2.contains("api_key"));

        // Retrieve the secret
        let value = store2.get("api_key").unwrap();
        assert_eq!(value, "secret123");
    }

    #[test]
    fn test_set_and_get() {
        let temp_dir = tempdir().unwrap();
        let vault_path = temp_dir.path().join("vault.toml");
        let master_password = "master123";

        let mut store = SecretStore::open(&vault_path, master_password).unwrap();
        store.set("db_password", "s3cr3t!", &["dev", "database"]).unwrap();

        let retrieved = store.get("db_password").unwrap();
        assert_eq!(retrieved, "s3cr3t!");
    }

    #[test]
    fn test_list_secrets() {
        let temp_dir = tempdir().unwrap();
        let vault_path = temp_dir.path().join("vault.toml");
        let master_password = "pass";

        let mut store = SecretStore::open(&vault_path, master_password).unwrap();
        store.set("key1", "value1", &["tag1"]).unwrap();
        store.set("key2", "value2", &["tag2", "tag3"]).unwrap();

        let list = store.list();
        assert_eq!(list.len(), 2);

        let key1_entry = list.iter().find(|(k, _)| *k == "key1").unwrap();
        assert_eq!(key1_entry.1, vec!["tag1"]);

        let key2_entry = list.iter().find(|(k, _)| *k == "key2").unwrap();
        assert_eq!(key2_entry.1, vec!["tag2", "tag3"]);
    }

    #[test]
    fn test_remove_secret() {
        let temp_dir = tempdir().unwrap();
        let vault_path = temp_dir.path().join("vault.toml");
        let master_password = "pwd";

        let mut store = SecretStore::open(&vault_path, master_password).unwrap();
        store.set("token", "abc123", &[]).unwrap();
        assert!(store.contains("token"));

        store.remove("token").unwrap();
        assert!(!store.contains("token"));
    }

    #[test]
    fn test_wrong_password() {
        let temp_dir = tempdir().unwrap();
        let vault_path = temp_dir.path().join("vault.toml");
        let correct_password = "correct";
        let wrong_password = "wrong";

        let mut store = SecretStore::open(&vault_path, correct_password).unwrap();
        store.set("secret", "value", &[]).unwrap();
        store.save().unwrap();

        let store2 = SecretStore::open(&vault_path, wrong_password).unwrap();
        let result = store2.get("secret");
        assert!(result.is_err());
    }

    #[test]
    fn test_persistence() {
        let temp_dir = tempdir().unwrap();
        let vault_path = temp_dir.path().join("vault.toml");
        let password = "pwd";

        // Create and populate
        let mut store1 = SecretStore::open(&vault_path, password).unwrap();
        store1.set("k1", "v1", &["t1"]).unwrap();
        store1.set("k2", "v2", &["t2"]).unwrap();
        store1.save().unwrap();

        // Load in a new instance
        let store2 = SecretStore::open(&vault_path, password).unwrap();
        assert_eq!(store2.len(), 2);
        assert_eq!(store2.get("k1").unwrap(), "v1");
        assert_eq!(store2.get("k2").unwrap(), "v2");

        // Modify and save again
        let mut store3 = store2.clone();
        store3.set("k3", "v3", &[]).unwrap();
        store3.remove("k1").unwrap();
        store3.save().unwrap();

        // Final load
        let store4 = SecretStore::open(&vault_path, password).unwrap();
        assert_eq!(store4.len(), 2);
        assert!(!store4.contains("k1"));
        assert!(store4.contains("k2"));
        assert!(store4.contains("k3"));
    }
}
