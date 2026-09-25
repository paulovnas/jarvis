//! Persistent desktop Secret Service vault shared by providers and MCP/Context7.
//! No kernel-only cache or plaintext fallback: an inaccessible wallet is an error.

use crate::data_dir::{self, Profile};
use keyring::{Entry, Error};

pub(crate) const RECOVERY_MESSAGE: &str = "Não foi possível acessar o cofre de credenciais do Linux. Ative um serviço Secret Service (como GNOME Keyring ou KeePassXC) na sessão, desbloqueie o cofre e tente novamente.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VaultError {
    Unavailable,
    NotFound,
    OperationFailed,
}

fn translate(error: Error) -> VaultError {
    // Never propagate backend errors: some variants carry credential bytes.
    match error {
        Error::NoEntry => VaultError::NotFound,
        Error::PlatformFailure(_) => VaultError::Unavailable,
        _ => VaultError::OperationFailed,
    }
}

fn service(profile: Profile, namespace: &str) -> String {
    let application = match profile {
        Profile::Production => "com.foxtag.jarvis",
        Profile::Development => "com.foxtag.jarvis.dev",
    };
    format!("{application}.{namespace}")
}

fn entry(namespace: &str, key: &str) -> Result<Entry, VaultError> {
    Entry::new(&service(data_dir::profile(), namespace), key).map_err(translate)
}

pub(crate) fn load(namespace: &str, key: &str) -> Result<Vec<u8>, VaultError> {
    entry(namespace, key)?.get_secret().map_err(translate)
}

pub(crate) fn store(namespace: &str, key: &str, value: &[u8]) -> Result<(), VaultError> {
    entry(namespace, key)?.set_secret(value).map_err(translate)
}

pub(crate) fn delete(namespace: &str, key: &str) -> Result<(), VaultError> {
    match entry(namespace, key)?.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(error) => Err(translate(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_and_consumers_use_distinct_persistent_services() {
        let services = [
            service(Profile::Production, "provider-secrets"),
            service(Profile::Development, "provider-secrets"),
            service(Profile::Production, "mcp-secrets"),
            service(Profile::Development, "mcp-secrets"),
        ];
        assert_eq!(
            services
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            4
        );
        assert_eq!(services[0], "com.foxtag.jarvis.provider-secrets");
    }

    #[test]
    fn absent_locked_and_unavailable_wallets_are_distinct_from_success() {
        assert_eq!(translate(Error::NoEntry), VaultError::NotFound);
        assert_eq!(
            translate(Error::NoStorageAccess(Box::new(std::io::Error::other(
                "locked"
            )))),
            VaultError::OperationFailed
        );
        assert_eq!(
            translate(Error::PlatformFailure(Box::new(std::io::Error::other(
                "no bus"
            )))),
            VaultError::Unavailable
        );
        assert_eq!(
            translate(Error::BadEncoding(b"secret".to_vec())),
            VaultError::OperationFailed
        );
    }

    // Run only against a disposable D-Bus session and wallet (see the Linux guide).
    #[test]
    #[ignore = "requires an isolated, unlocked Secret Service wallet"]
    fn linux_secret_service_round_trip() {
        use crate::mcp::Secrets;
        use crate::openai_codex::{
            CodexCredential, KeychainSecretStore, SecretStore, SecretStoreError,
        };

        let key = format!("test-{}", crate::library::new_id().unwrap());
        let mcp = crate::mcp::Keychain;
        assert_eq!(load("mcp-secrets", &key), Err(VaultError::NotFound));
        mcp.store(&key, "ctx7sk-synthetic-café").unwrap();
        assert_eq!(mcp.load(&key).unwrap(), "ctx7sk-synthetic-café");
        assert_eq!(load("provider-secrets", &key), Err(VaultError::NotFound));
        let other_profile = if data_dir::profile() == Profile::Development {
            Profile::Production
        } else {
            Profile::Development
        };
        assert!(matches!(
            Entry::new(&service(other_profile, "mcp-secrets"), &key)
                .unwrap()
                .get_secret(),
            Err(Error::NoEntry)
        ));
        mcp.store(&key, "rotated").unwrap();
        assert_eq!(mcp.load(&key).unwrap(), "rotated");
        mcp.delete(&key).unwrap();
        mcp.delete(&key).unwrap();
        assert_eq!(load("mcp-secrets", &key), Err(VaultError::NotFound));

        for prefix in ["openai-codex", "antigravity", "custom"] {
            let alias = format!("{prefix}-{key}");
            let credential = CodexCredential::new(
                "synthetic-token",
                "synthetic-refresh",
                i64::MAX,
                "test-account",
                None,
                None,
            );
            KeychainSecretStore.store(&alias, &credential).unwrap();
            assert!(KeychainSecretStore.load(&alias).unwrap() == credential);
            let rotated = CodexCredential::new(
                "rotated-token",
                "rotated-refresh",
                i64::MAX,
                "test-account",
                None,
                None,
            );
            KeychainSecretStore.store(&alias, &rotated).unwrap();
            assert!(KeychainSecretStore.load(&alias).unwrap() == rotated);
            KeychainSecretStore.remove(&alias).unwrap();
            KeychainSecretStore.remove(&alias).unwrap();
            assert!(matches!(
                KeychainSecretStore.load(&alias),
                Err(SecretStoreError::Missing)
            ));
        }
    }

    #[test]
    #[ignore = "requires an isolated D-Bus session without a Secret Service"]
    fn linux_secret_service_unavailable() {
        let key = "synthetic-unavailable-test";
        for result in [
            load("mcp-secrets", key).map(|_| ()),
            store("mcp-secrets", key, b"synthetic"),
            delete("mcp-secrets", key),
        ] {
            assert!(matches!(
                result,
                Err(VaultError::Unavailable | VaultError::OperationFailed)
            ));
        }
    }

    #[test]
    #[ignore = "requires the isolated wallet restart test script"]
    fn linux_secret_service_persistence() {
        let namespace = "test-persistence";
        let key = "synthetic-persistent-key";
        match std::env::var("JARVIS_KEYRING_TEST_PHASE").as_deref() {
            Ok("store") => store(namespace, key, b"synthetic-persistent-value").unwrap(),
            Ok("locked") => {
                for result in [
                    load(namespace, key).map(|_| ()),
                    store(namespace, key, b"replacement"),
                    delete(namespace, key),
                ] {
                    assert!(matches!(
                        result,
                        Err(VaultError::Unavailable | VaultError::OperationFailed)
                    ));
                }
            }
            Ok("reopened") => {
                assert_eq!(load(namespace, key).unwrap(), b"synthetic-persistent-value");
                delete(namespace, key).unwrap();
            }
            _ => panic!("Run scripts/check-linux-keyring.sh with an isolated wallet"),
        }
    }
}
