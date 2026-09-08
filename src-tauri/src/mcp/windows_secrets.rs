//! MCP/Context7 secret access. Delegates to the shared Windows DPAPI vault.
//!
//! The namespace `mcp-secrets` is preserved verbatim so a key saved before this
//! refactor still decrypts after it: the on-disk directory and per-key hashing
//! are unchanged.
use super::{storage_error, McpError};

const NAMESPACE: &str = "mcp-secrets";

fn translate(error: crate::secrets::VaultError) -> McpError {
    // A missing MCP secret is not a hard storage failure: callers treat an
    // absent config the same as an unreadable one. Keep the existing message.
    let _ = error;
    storage_error()
}

pub(super) fn load(key: &str) -> Result<String, McpError> {
    let bytes = crate::secrets::load(NAMESPACE, key).map_err(translate)?;
    String::from_utf8(bytes).map_err(|_| storage_error())
}

pub(super) fn store(key: &str, value: &str) -> Result<(), McpError> {
    crate::secrets::store(NAMESPACE, key, value.as_bytes()).map_err(translate)
}

pub(super) fn delete(key: &str) -> Result<(), McpError> {
    crate::secrets::delete(NAMESPACE, key).map_err(translate)
}
