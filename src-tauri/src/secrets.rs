//! Shared Windows secret vault backed by DPAPI in the user's scope.
//!
//! One primitive serves every consumer (MCP/Context7 configs and the provider
//! credentials for OpenAI Codex, Antigravity and Custom): blobs are encrypted
//! with `CryptProtectData` and stored under the active Jarvis data root, keyed by a
//! hash of the logical key so the plaintext key never names a file. The blob is
//! bound to the Windows user: copying it to another machine or account does not
//! decrypt it, which is the intended property, not a migration path.
//!
//! macOS uses the Keychain directly in each consumer and does not compile this
//! module; other platforms have no secure backend here.
#![cfg(target_os = "windows")]

use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::PathBuf, slice};
use tempfile::NamedTempFile;
use windows_sys::Win32::{
    Foundation::{LocalFree, HLOCAL},
    Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VaultError {
    /// The home profile could not be resolved.
    Unavailable,
    /// No secret exists for this key.
    NotFound,
    /// The DPAPI or filesystem operation failed.
    OperationFailed,
}

fn directory(namespace: &str) -> Result<PathBuf, VaultError> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|home| crate::data_dir::root(&PathBuf::from(home)).join(namespace))
        .ok_or(VaultError::Unavailable)
}

fn path(namespace: &str, key: &str) -> Result<PathBuf, VaultError> {
    Ok(directory(namespace)?.join(format!("{:x}.bin", Sha256::digest(key.as_bytes()))))
}

fn copy_blob(blob: &CRYPT_INTEGER_BLOB) -> Result<Vec<u8>, VaultError> {
    if blob.cbData == 0 {
        return Ok(Vec::new());
    }
    if blob.pbData.is_null() {
        return Err(VaultError::OperationFailed);
    }
    Ok(unsafe { slice::from_raw_parts(blob.pbData, blob.cbData as usize) }.to_vec())
}

fn protect(value: &[u8]) -> Result<Vec<u8>, VaultError> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(value.len()).map_err(|_| VaultError::OperationFailed)?,
        pbData: value.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let result = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if result == 0 {
        return Err(VaultError::OperationFailed);
    }
    let value = copy_blob(&output);
    unsafe {
        if !output.pbData.is_null() {
            LocalFree(output.pbData as HLOCAL);
        }
    }
    value
}

fn unprotect(value: &[u8]) -> Result<Vec<u8>, VaultError> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(value.len()).map_err(|_| VaultError::OperationFailed)?,
        pbData: value.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let result = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if result == 0 {
        return Err(VaultError::OperationFailed);
    }
    let value = copy_blob(&output);
    unsafe {
        if !output.pbData.is_null() {
            LocalFree(output.pbData as HLOCAL);
        }
    }
    value
}

/// Read the decrypted blob for `key`, or `VaultError::NotFound` when absent.
pub(crate) fn load(namespace: &str, key: &str) -> Result<Vec<u8>, VaultError> {
    let encrypted = match fs::read(path(namespace, key)?) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(VaultError::NotFound)
        }
        Err(_) => return Err(VaultError::OperationFailed),
    };
    unprotect(&encrypted)
}

/// Encrypt and atomically persist `value` for `key`, replacing any prior blob.
pub(crate) fn store(namespace: &str, key: &str, value: &[u8]) -> Result<(), VaultError> {
    let path = path(namespace, key)?;
    let parent = path.parent().ok_or(VaultError::OperationFailed)?;
    fs::create_dir_all(parent).map_err(|_| VaultError::OperationFailed)?;
    let encrypted = protect(value)?;
    let mut file = NamedTempFile::new_in(parent).map_err(|_| VaultError::OperationFailed)?;
    file.write_all(&encrypted)
        .map_err(|_| VaultError::OperationFailed)?;
    file.as_file()
        .sync_all()
        .map_err(|_| VaultError::OperationFailed)?;
    file.persist(path)
        .map(|_| ())
        .map_err(|_| VaultError::OperationFailed)
}

/// Remove the blob for `key`. Absent keys succeed so disconnect stays idempotent.
pub(crate) fn delete(namespace: &str, key: &str) -> Result<(), VaultError> {
    match fs::remove_file(path(namespace, key)?) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(VaultError::OperationFailed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_namespace() -> String {
        format!("test-{}", crate::library::new_id().unwrap())
    }

    #[test]
    fn dpapi_round_trip_keeps_secret_out_of_the_ciphertext() {
        let secret = "ctx7sk-café-secret";
        let encrypted = protect(secret.as_bytes()).unwrap();
        assert_ne!(encrypted, secret.as_bytes());
        assert_eq!(unprotect(&encrypted).unwrap(), secret.as_bytes());
    }

    #[test]
    fn vault_isolates_namespaces_and_replaces_then_deletes_idempotently() {
        let ns = unique_namespace();
        let other = unique_namespace();
        assert_eq!(load(&ns, "alias"), Err(VaultError::NotFound));

        store(&ns, "alias", b"first").unwrap();
        assert_eq!(load(&ns, "alias").unwrap(), b"first");
        // The same key under another namespace is a distinct, absent secret.
        assert_eq!(load(&other, "alias"), Err(VaultError::NotFound));

        store(&ns, "alias", b"second").unwrap();
        assert_eq!(load(&ns, "alias").unwrap(), b"second");

        delete(&ns, "alias").unwrap();
        assert_eq!(load(&ns, "alias"), Err(VaultError::NotFound));
        // Deleting an already-absent key is not an error.
        delete(&ns, "alias").unwrap();

        let _ = fs::remove_dir_all(directory(&ns).unwrap());
        let _ = fs::remove_dir_all(directory(&other).unwrap());
    }
}
