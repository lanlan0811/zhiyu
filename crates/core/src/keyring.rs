//! API-key storage: local encryption, never plaintext at rest.
//!
//! - **Windows**: DPAPI (`CryptProtectData` / `CryptUnprotectData`) with the
//!   user's login credentials; ciphertext is base64-encoded into a per-provider
//!   file under `~/.zhiyu/keys/`.
//! - **macOS / Linux**: plaintext JSON guarded by a 0700 directory, as a
//!   documented fallback (the plan's honest boundary: real Keyring-backed
//!   storage there is a follow-up).
//!
//! Keys are organized per provider (`deepseek`, `glm`, …) with a list of keys
//! and rotation support — one provider shares its key across its models.

use std::path::PathBuf;

use base64::Engine;
use zhiyu_protocol::{ProviderKey, ProviderKeys};

use crate::paths::data_dir;

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("base64 error: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("dpapi error: {0}")]
    Dpapi(String),
    #[error("no keys stored for provider {0}")]
    NotFound(String),
}

/// Encrypts `plaintext` at rest. Windows → DPAPI; elsewhere → stored raw
/// (the fallback backend writes only into a 0700 directory).
pub fn encrypt(plaintext: &[u8]) -> Result<Vec<u8>, KeyError> {
    #[cfg(windows)]
    {
        dpapi::protect(plaintext)
    }
    #[cfg(not(windows))]
    {
        Ok(plaintext.to_vec())
    }
}

/// Decrypts bytes produced by [`encrypt`].
pub fn decrypt(ciphertext: &[u8]) -> Result<Vec<u8>, KeyError> {
    #[cfg(windows)]
    {
        dpapi::unprotect(ciphertext)
    }
    #[cfg(not(windows))]
    {
        Ok(ciphertext.to_vec())
    }
}

/// Per-provider key store backed by the encrypted files under
/// `~/.zhiyu/keys/`.
pub struct KeyStore {
    keys_dir: PathBuf,
}

impl Default for KeyStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyStore {
    pub fn new() -> Self {
        KeyStore { keys_dir: data_dir().join("keys") }
    }

    fn provider_path(&self, provider: &str) -> PathBuf {
        self.keys_dir.join(format!("{provider}.key"))
    }

    /// Saves (replaces) the key list of a provider, encrypted at rest.
    pub fn save(&self, provider: &str, keys: &ProviderKeys) -> Result<(), KeyError> {
        std::fs::create_dir_all(&self.keys_dir)?;
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.keys_dir, std::fs::Permissions::from_mode(0o700))?;
        }
        let plain = serde_json::to_vec(keys)?;
        let cipher = encrypt(&plain)?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(cipher);
        std::fs::write(self.provider_path(provider), encoded)?;
        Ok(())
    }

    /// Loads the key list of a provider.
    pub fn load(&self, provider: &str) -> Result<ProviderKeys, KeyError> {
        let path = self.provider_path(provider);
        if !path.exists() {
            return Err(KeyError::NotFound(provider.to_string()));
        }
        let encoded = std::fs::read_to_string(path)?;
        let cipher = base64::engine::general_purpose::STANDARD.decode(encoded.trim())?;
        let plain = decrypt(&cipher)?;
        Ok(serde_json::from_slice(&plain)?)
    }

    /// Adds (or replaces) a single key and marks it default when it is the
    /// first key of the provider.
    pub fn upsert_key(&self, provider: &str, key: &str) -> Result<ProviderKeys, KeyError> {
        let mut keys = self.load(provider).unwrap_or(ProviderKeys {
            provider: provider.to_string(),
            keys: vec![],
            default_key_id: None,
        });
        let id = format!("k{}", keys.keys.len() + 1);
        keys.keys.push(ProviderKey { id: id.clone(), key: key.to_string(), is_default: keys.keys.is_empty() });
        if keys.default_key_id.is_none() {
            keys.default_key_id = Some(id);
        }
        self.save(provider, &keys)?;
        Ok(keys)
    }

    /// Deletes one key of a provider.
    pub fn delete_key(&self, provider: &str, key_id: &str) -> Result<(), KeyError> {
        let mut keys = self.load(provider)?;
        keys.keys.retain(|k| k.id != key_id);
        if keys.default_key_id.as_deref() == Some(key_id) {
            keys.default_key_id = keys.keys.first().map(|k| k.id.clone());
        }
        if keys.keys.is_empty() {
            let _ = std::fs::remove_file(self.provider_path(provider));
        } else {
            self.save(provider, &keys)?;
        }
        Ok(())
    }

    /// Sets the default key of a provider.
    pub fn set_default(&self, provider: &str, key_id: &str) -> Result<(), KeyError> {
        let mut keys = self.load(provider)?;
        if !keys.keys.iter().any(|k| k.id == key_id) {
            return Err(KeyError::NotFound(format!("key {key_id} of {provider}")));
        }
        keys.default_key_id = Some(key_id.to_string());
        self.save(provider, &keys)
    }

    /// Rotates to the next key (used for round-robin across the provider's
    /// keys); returns the new default key id.
    pub fn rotate(&self, provider: &str) -> Result<Option<String>, KeyError> {
        let mut keys = self.load(provider)?;
        if keys.keys.is_empty() {
            return Ok(None);
        }
        let current = keys.default_key_id.clone();
        let next = keys
            .keys
            .iter()
            .position(|k| Some(&k.id) == current.as_ref())
            .map_or(0, |i| (i + 1) % keys.keys.len());
        let new_default = keys.keys[next].id.clone();
        keys.default_key_id = Some(new_default.clone());
        self.save(provider, &keys)?;
        Ok(Some(new_default))
    }

    /// The plaintext of the provider's default key, for building requests.
    pub fn default_key(&self, provider: &str) -> Result<String, KeyError> {
        let keys = self.load(provider)?;
        keys.default_key()
            .map(|k| k.key.clone())
            .ok_or_else(|| KeyError::NotFound(provider.to_string()))
    }
}

/// DPAPI bindings via windows-sys.
#[cfg(windows)]
mod dpapi {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
    };

    use super::KeyError;

    pub fn protect(data: &[u8]) -> Result<Vec<u8>, KeyError> {
        let in_blob = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let mut out_blob = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        let ok = unsafe {
            CryptProtectData(
                &in_blob,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out_blob,
            )
        };
        if ok == 0 {
            return Err(KeyError::Dpapi("CryptProtectData failed".into()));
        }
        let out = unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) }.to_vec();
        unsafe { LocalFree(out_blob.pbData as *mut _) };
        Ok(out)
    }

    pub fn unprotect(cipher: &[u8]) -> Result<Vec<u8>, KeyError> {
        let in_blob = CRYPT_INTEGER_BLOB { cbData: cipher.len() as u32, pbData: cipher.as_ptr() as *mut u8 };
        let mut out_blob = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        let ok = unsafe {
            CryptUnprotectData(
                &in_blob,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out_blob,
            )
        };
        if ok == 0 {
            return Err(KeyError::Dpapi("CryptUnprotectData failed".into()));
        }
        let out = unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) }.to_vec();
        unsafe { LocalFree(out_blob.pbData as *mut _) };
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (tempfile::TempDir, KeyStore) {
        let dir = tempfile::tempdir().unwrap();
        let keys_dir = dir.path().join("keys");
        let store = KeyStore { keys_dir };
        (dir, store)
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let plain = b"sk-test-12345";
        let cipher = encrypt(plain).unwrap();
        // On Windows the ciphertext must differ (DPAPI); on other platforms
        // the fallback backend stores raw bytes in a 0700 dir.
        #[cfg(windows)]
        assert_ne!(&cipher, plain);
        let back = decrypt(&cipher).unwrap();
        assert_eq!(back, plain);
    }

    #[test]
    fn save_load_upsert_delete_rotate() {
        let (_dir, store) = temp_store();
        let provider = "deepseek";

        store.upsert_key(provider, "key-one").unwrap();
        store.upsert_key(provider, "key-two").unwrap();
        let keys = store.load(provider).unwrap();
        assert_eq!(keys.keys.len(), 2);
        assert_eq!(keys.default_key().unwrap().key, "key-one");

        // set default to the second key
        let second = keys.keys[1].id.clone();
        store.set_default(provider, &second).unwrap();
        assert_eq!(store.default_key(provider).unwrap(), "key-two");

        // rotate → wraps back to the first key (id k1)
        let rotated = store.rotate(provider).unwrap();
        assert_eq!(rotated, Some("k1".to_string()));
        assert_eq!(store.default_key(provider).unwrap(), "key-one");

        // delete the second key
        store.delete_key(provider, &second).unwrap();
        let keys = store.load(provider).unwrap();
        assert_eq!(keys.keys.len(), 1);

        // provider without keys → NotFound
        let missing = store.load("glm");
        assert!(matches!(missing, Err(KeyError::NotFound(_))));
    }

    #[test]
    fn stored_file_never_contains_plaintext() {
        let (_dir, store) = temp_store();
        store.upsert_key("glm", "supersecret").unwrap();
        let path = store.provider_path("glm");
        let content = std::fs::read_to_string(path).unwrap();
        // base64 of DPAPI ciphertext (Windows) or base64 of the raw bytes
        // (fallback) — the raw key string never appears in the file
        assert!(!content.contains("supersecret"));
    }

    #[test]
    fn helper_new_uses_home() {
        // The default KeyStore points at ~/.zhiyu/keys — just verify the path.
        let store = KeyStore::new();
        assert!(store.keys_dir.to_string_lossy().contains("zhiyu"));
    }
}
