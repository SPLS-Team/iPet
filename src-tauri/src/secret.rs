//! Local-machine secret encryption for at-rest credentials.
//!
//! Encrypts short strings (API keys, header values) with ChaCha20-Poly1305
//! keyed by a 32-byte secret stored in the app data directory.
//!
//! Threat model: stops casual reads of `ipet.sqlite3` (e.g. someone peeking at
//! the file on disk, or it ending up in a backup). It is NOT a substitute for
//! an OS keychain — anyone with read access to the data dir gets the key
//! along with the ciphertext. For stronger protection plumb in Tauri
//! stronghold / OS keyring; this is the local fallback.

use crate::app_error::{AppError, AppResult};
use base64::Engine;
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const ENVELOPE_PREFIX: &str = "enc:v1:";

/// 32-byte symmetric key persisted under `<data_dir>/secret.key`.
#[derive(Clone)]
pub struct MachineKey {
    cipher: ChaCha20Poly1305,
    #[allow(dead_code)]
    path: PathBuf,
}

impl MachineKey {
    /// Read the key file, or generate + persist a fresh 32-byte key on first
    /// run. Restrictive POSIX permissions are best-effort (no-op on Windows
    /// since the data dir already lives under the user profile).
    pub fn load_or_generate(data_dir: &Path) -> AppResult<Self> {
        let path = data_dir.join("secret.key");

        let bytes = if path.exists() {
            let raw = fs::read(&path)?;
            if raw.len() != 32 {
                return Err(AppError::Config(format!(
                    "secret.key 大小异常：期望 32 字节，得到 {}",
                    raw.len()
                )));
            }
            let mut buf = [0u8; 32];
            buf.copy_from_slice(&raw);
            buf
        } else {
            let key = ChaCha20Poly1305::generate_key(&mut OsRng);
            let mut buf = [0u8; 32];
            buf.copy_from_slice(key.as_slice());
            write_locked_down(&path, &buf)?;
            buf
        };

        let cipher = ChaCha20Poly1305::new(Key::from_slice(&bytes));
        Ok(Self { cipher, path })
    }

    /// Path to the on-disk key file. Useful for diagnostics.
    #[allow(dead_code)]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Wrap `plaintext` in an opaque envelope. Random nonce per call so the
    /// same value encrypts differently each time (no equality leaks).
    pub fn encrypt(&self, plaintext: &str) -> AppResult<String> {
        let nonce_bytes: [u8; 12] = {
            let n = ChaCha20Poly1305::generate_nonce(&mut OsRng);
            let mut buf = [0u8; 12];
            buf.copy_from_slice(n.as_slice());
            buf
        };
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|err| AppError::Config(format!("加密失败: {err}")))?;
        let b64 = base64::engine::general_purpose::STANDARD;
        Ok(format!(
            "{ENVELOPE_PREFIX}{}:{}",
            b64.encode(nonce_bytes),
            b64.encode(ciphertext)
        ))
    }

    /// Reverse of [`encrypt`]. Returns `Ok(None)` if `envelope` doesn't carry
    /// the encryption prefix — that's the migration path for values written
    /// before this module existed.
    pub fn decrypt(&self, envelope: &str) -> AppResult<Option<String>> {
        let Some(body) = envelope.strip_prefix(ENVELOPE_PREFIX) else {
            return Ok(None);
        };
        let (nonce_b64, cipher_b64) = body
            .split_once(':')
            .ok_or_else(|| AppError::Config("加密信封格式无效".to_string()))?;
        let b64 = base64::engine::general_purpose::STANDARD;
        let nonce_bytes = b64
            .decode(nonce_b64)
            .map_err(|err| AppError::Config(format!("加密信封解析失败: {err}")))?;
        if nonce_bytes.len() != 12 {
            return Err(AppError::Config("nonce 长度异常".to_string()));
        }
        let ciphertext = b64
            .decode(cipher_b64)
            .map_err(|err| AppError::Config(format!("加密信封解析失败: {err}")))?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let plain = self
            .cipher
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|err| AppError::Config(format!("解密失败（密钥变更或文件损坏）: {err}")))?;
        Ok(Some(
            String::from_utf8(plain).map_err(|err| AppError::Config(err.to_string()))?,
        ))
    }

    /// Convenience: returns `value` decrypted if it carries the envelope,
    /// otherwise the original (treated as a legacy plaintext value). Use this
    /// at the load boundary so old DBs keep working.
    pub fn decrypt_or_passthrough(&self, value: &str) -> AppResult<String> {
        match self.decrypt(value)? {
            Some(plain) => Ok(plain),
            None => Ok(value.to_string()),
        }
    }
}

/// Best-effort restrictive write. On unix we set mode 0o600; on windows we
/// rely on the app data dir already being per-user.
fn write_locked_down(path: &Path, bytes: &[u8]) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.flush()?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        let _ = fs::set_permissions(path, perms);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn generates_and_reloads_same_key() {
        let dir = TempDir::new("secret");
        let k1 = MachineKey::load_or_generate(dir.path()).unwrap();
        let k2 = MachineKey::load_or_generate(dir.path()).unwrap();
        let cipher = k1.encrypt("hello").unwrap();
        let plain = k2.decrypt(&cipher).unwrap().unwrap();
        assert_eq!(plain, "hello");
    }

    #[test]
    fn round_trips_unicode_and_long_values() {
        let dir = TempDir::new("secret-unicode");
        let key = MachineKey::load_or_generate(dir.path()).unwrap();
        for sample in [
            "",
            "sk-1234567890ABCDEF",
            "包含中文与 emoji 🎉 的密钥",
            &"x".repeat(4096),
        ] {
            let env = key.encrypt(sample).unwrap();
            assert!(env.starts_with("enc:v1:"));
            let back = key.decrypt(&env).unwrap().unwrap();
            assert_eq!(back, sample);
        }
    }

    #[test]
    fn encryption_uses_fresh_nonce_each_call() {
        let dir = TempDir::new("secret-nonce");
        let key = MachineKey::load_or_generate(dir.path()).unwrap();
        let a = key.encrypt("same input").unwrap();
        let b = key.encrypt("same input").unwrap();
        assert_ne!(a, b, "envelopes must differ for the same plaintext");
    }

    #[test]
    fn decrypt_passthrough_for_legacy_plaintext() {
        let dir = TempDir::new("secret-legacy");
        let key = MachineKey::load_or_generate(dir.path()).unwrap();
        assert!(key.decrypt("plain api key").unwrap().is_none());
        assert_eq!(
            key.decrypt_or_passthrough("plain api key").unwrap(),
            "plain api key"
        );
    }

    #[test]
    fn malformed_envelope_errors() {
        let dir = TempDir::new("secret-bad");
        let key = MachineKey::load_or_generate(dir.path()).unwrap();
        assert!(key.decrypt("enc:v1:").is_err());
        assert!(key.decrypt("enc:v1:not-base64::also-not").is_err());
    }
}
