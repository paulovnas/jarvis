use serde::{Deserialize, Serialize};

use crate::persistence::{self, PersistenceError, ProviderAccountRecord};

pub(crate) mod antigravity;
pub(crate) mod custom;
mod reauthorization;
pub(crate) mod usage;

pub(crate) const OPENAI_CODEX_ALIAS_PREFIX: &str = "openai-codex-";
#[cfg(target_os = "macos")]
pub(crate) const KEYCHAIN_SERVICE: &str = "com.foxtag.jarvis.openai-codex";
pub(crate) const OPENAI_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api";
pub(crate) const OPENAI_CODEX_CLIENT_VERSION: &str = "0.153.0";
const OPENAI_CODEX_PROFILE_CLAIM: &str = "https://api.openai.com/profile";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct ProviderModel {
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(rename = "reasoningLevels")]
    pub(crate) reasoning_levels: Vec<String>,
    #[serde(rename = "defaultReasoningLevel")]
    pub(crate) default_reasoning_level: Option<String>,
    #[serde(default, rename = "contextWindow")]
    pub(crate) context_window: Option<u64>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ProviderAccountType {
    Personal,
    Enterprise,
    Unknown,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct ProviderAccount {
    pub(crate) alias: String,
    pub(crate) enabled: bool,
    #[serde(rename = "showUsage")]
    pub(crate) show_usage: bool,
    #[serde(rename = "showThirdPartyUsage")]
    pub(crate) show_third_party_usage: bool,
    #[serde(rename = "providerKind")]
    pub(crate) provider_kind: String,
    #[serde(rename = "createdAt")]
    pub(crate) created_at: i64,
    pub(crate) email: Option<String>,
    #[serde(rename = "accountType")]
    pub(crate) account_type: ProviderAccountType,
    pub(crate) models: Vec<ProviderModel>,
    #[serde(rename = "modelsAvailable")]
    pub(crate) models_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) custom: Option<custom::Config>,
}

impl ProviderAccount {
    fn from_record(
        record: ProviderAccountRecord,
        credential: Option<&CodexCredential>,
        models: Vec<ProviderModel>,
        models_available: bool,
    ) -> Self {
        Self {
            alias: record.alias,
            enabled: record.enabled,
            show_usage: record.show_usage,
            show_third_party_usage: record.show_third_party_usage,
            provider_kind: record.provider_kind,
            created_at: record.created_at,
            email: credential.and_then(|value| value.email.clone()),
            account_type: credential
                .and_then(|value| value.plan_type.as_deref())
                .map_or(ProviderAccountType::Unknown, classify_account_type),
            models,
            models_available,
            custom: None,
        }
    }
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct CodexCredential {
    pub(crate) version: u8,
    pub(crate) access: String,
    pub(crate) refresh: String,
    pub(crate) expires: i64,
    #[serde(rename = "accountId")]
    pub(crate) account_id: String,
    #[serde(default)]
    pub(crate) email: Option<String>,
    #[serde(default, rename = "planType")]
    pub(crate) plan_type: Option<String>,
    #[serde(default, rename = "projectId", skip_serializing_if = "Option::is_none")]
    pub(crate) project_id: Option<String>,
    #[serde(skip)]
    pub(crate) antigravity_models: std::collections::BTreeMap<String, serde_json::Value>,
    #[serde(skip)]
    pub(crate) antigravity_endpoint: Option<String>,
    #[serde(skip)]
    pub(crate) custom: Option<custom::Config>,
}

impl CodexCredential {
    pub(crate) fn new(
        access: impl Into<String>,
        refresh: impl Into<String>,
        expires: i64,
        account_id: impl Into<String>,
        email: Option<String>,
        plan_type: Option<String>,
    ) -> Self {
        Self {
            version: 1,
            access: access.into(),
            refresh: refresh.into(),
            expires,
            account_id: account_id.into(),
            email,
            plan_type,
            project_id: None,
            antigravity_models: Default::default(),
            antigravity_endpoint: None,
            custom: None,
        }
    }
}
fn classify_account_type(plan_type: &str) -> ProviderAccountType {
    const ENTERPRISE_PLANS: [&str; 10] = [
        "business",
        "team",
        "enterprise",
        "edu",
        "education",
        "teacher",
        "teachers",
        "health",
        "gov",
        "government",
    ];
    const PERSONAL_PLANS: [&str; 5] = ["plus", "pro", "prolite", "free", "go"];

    let mut personal = false;
    for token in plan_type
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
    {
        if ENTERPRISE_PLANS.contains(&token) {
            return ProviderAccountType::Enterprise;
        }
        personal |= PERSONAL_PLANS.contains(&token);
    }
    if personal {
        ProviderAccountType::Personal
    } else {
        ProviderAccountType::Unknown
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AliasValidationError {
    InvalidSuffix,
    InvalidAlias,
}

pub(crate) fn validate_alias_suffix(suffix: &str) -> Result<(), AliasValidationError> {
    let bytes = suffix.as_bytes();
    if !(1..=32).contains(&bytes.len()) {
        return Err(AliasValidationError::InvalidSuffix);
    }

    let mut previous_was_hyphen = false;
    for (index, byte) in bytes.iter().copied().enumerate() {
        let is_lowercase_ascii = byte.is_ascii_lowercase();
        let is_ascii_digit = byte.is_ascii_digit();
        if byte == b'-' {
            if index == 0 || previous_was_hyphen {
                return Err(AliasValidationError::InvalidSuffix);
            }
            previous_was_hyphen = true;
        } else if is_lowercase_ascii || is_ascii_digit {
            previous_was_hyphen = false;
        } else {
            return Err(AliasValidationError::InvalidSuffix);
        }
    }

    if previous_was_hyphen {
        return Err(AliasValidationError::InvalidSuffix);
    }
    Ok(())
}

pub(crate) fn validate_provider_alias(alias: &str) -> Result<(), AliasValidationError> {
    let suffix = alias
        .strip_prefix(OPENAI_CODEX_ALIAS_PREFIX)
        .or_else(|| alias.strip_prefix("antigravity-"))
        .ok_or(AliasValidationError::InvalidAlias)?;
    validate_alias_suffix(suffix).map_err(|_| AliasValidationError::InvalidAlias)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SecretStoreError {
    #[cfg(not(target_os = "macos"))]
    Unavailable,
    #[cfg(any(target_os = "macos", target_os = "windows", test))]
    OperationFailed,
    #[cfg(test)]
    Missing,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    InvalidCredential,
}

pub(crate) trait SecretStore: Send + Sync {
    fn load(&self, alias: &str) -> Result<CodexCredential, SecretStoreError>;
    fn store(&self, alias: &str, credential: &CodexCredential) -> Result<(), SecretStoreError>;
    fn remove(&self, alias: &str) -> Result<(), SecretStoreError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderAccountError {
    InvalidAlias(AliasValidationError),
    InvalidAccountId,
    DuplicateAccount,
    SecretStore(SecretStoreError),
    Database,
    SecretCleanup,
}

impl From<SecretStoreError> for ProviderAccountError {
    fn from(error: SecretStoreError) -> Self {
        Self::SecretStore(error)
    }
}

impl From<PersistenceError> for ProviderAccountError {
    fn from(_: PersistenceError) -> Self {
        Self::Database
    }
}

pub(crate) fn commit_provider_account(
    connection: &rusqlite::Connection,
    secret_store: &dyn SecretStore,
    alias: &str,
    credential: &CodexCredential,
) -> Result<ProviderAccount, ProviderAccountError> {
    validate_provider_alias(alias).map_err(ProviderAccountError::InvalidAlias)?;
    if credential.account_id.is_empty() {
        return Err(ProviderAccountError::InvalidAccountId);
    }
    if alias.starts_with("antigravity-") != credential.project_id.is_some() {
        return Err(ProviderAccountError::InvalidAccountId);
    }
    if persistence::provider_account_exists(connection, alias, &credential.account_id)? {
        return Err(ProviderAccountError::DuplicateAccount);
    }

    secret_store.store(alias, credential)?;
    let record =
        match persistence::insert_provider_account(connection, alias, &credential.account_id) {
            Ok(record) => record,
            Err(_) => {
                return match secret_store.remove(alias) {
                    Ok(()) => Err(ProviderAccountError::Database),
                    Err(_) => Err(ProviderAccountError::SecretCleanup),
                };
            }
        };

    Ok(ProviderAccount::from_record(
        record,
        Some(credential),
        Vec::new(),
        false,
    ))
}

pub(crate) fn disconnect_provider_account(
    connection: &rusqlite::Connection,
    secret_store: &dyn SecretStore,
    alias: &str,
) -> Result<(), ProviderAccountError> {
    custom::validate_alias(alias)
        .map_err(|_| ProviderAccountError::InvalidAlias(AliasValidationError::InvalidAlias))?;
    secret_store.remove(alias)?;
    persistence::delete_provider_account(connection, alias).map_err(Into::into)
}

#[cfg(target_os = "macos")]
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct KeychainSecretStore;

#[cfg(not(target_os = "macos"))]
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct KeychainSecretStore;

#[cfg(target_os = "macos")]
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn serialize_credential(credential: &CodexCredential) -> Result<Vec<u8>, SecretStoreError> {
    serde_json::to_vec(credential).map_err(|_| SecretStoreError::InvalidCredential)
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn deserialize_credential(value: &[u8]) -> Result<CodexCredential, SecretStoreError> {
    serde_json::from_slice(value).map_err(|_| SecretStoreError::InvalidCredential)
}

#[cfg(target_os = "macos")]
impl SecretStore for KeychainSecretStore {
    fn load(&self, alias: &str) -> Result<CodexCredential, SecretStoreError> {
        let value =
            security_framework::passwords::get_generic_password(secret_service(alias), alias)
                .map_err(|_| SecretStoreError::OperationFailed)?;
        deserialize_credential(&value)
    }

    fn store(&self, alias: &str, credential: &CodexCredential) -> Result<(), SecretStoreError> {
        let value = serialize_credential(credential)?;
        security_framework::passwords::set_generic_password(secret_service(alias), alias, &value)
            .map_err(|_| SecretStoreError::OperationFailed)
    }

    fn remove(&self, alias: &str) -> Result<(), SecretStoreError> {
        match security_framework::passwords::delete_generic_password(secret_service(alias), alias) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
            Err(_) => Err(SecretStoreError::OperationFailed),
        }
    }
}

#[cfg(target_os = "macos")]
fn secret_service(alias: &str) -> &'static str {
    if alias.starts_with("antigravity-") {
        "com.foxtag.jarvis.antigravity"
    } else {
        KEYCHAIN_SERVICE
    }
}

// Windows: provider credentials live in the shared DPAPI vault. One namespace
// keyed by alias covers OpenAI Codex, Antigravity and Custom — their aliases are
// already disjoint by prefix, so no per-provider service split is needed.
#[cfg(target_os = "windows")]
const PROVIDER_NAMESPACE: &str = "provider-secrets";

#[cfg(target_os = "windows")]
fn vault_error(error: crate::secrets::VaultError) -> SecretStoreError {
    match error {
        crate::secrets::VaultError::Unavailable => SecretStoreError::Unavailable,
        crate::secrets::VaultError::NotFound | crate::secrets::VaultError::OperationFailed => {
            SecretStoreError::OperationFailed
        }
    }
}

#[cfg(target_os = "windows")]
impl SecretStore for KeychainSecretStore {
    fn load(&self, alias: &str) -> Result<CodexCredential, SecretStoreError> {
        let value = crate::secrets::load(PROVIDER_NAMESPACE, alias).map_err(vault_error)?;
        deserialize_credential(&value)
    }

    fn store(&self, alias: &str, credential: &CodexCredential) -> Result<(), SecretStoreError> {
        let value = serialize_credential(credential)?;
        crate::secrets::store(PROVIDER_NAMESPACE, alias, &value).map_err(vault_error)
    }

    fn remove(&self, alias: &str) -> Result<(), SecretStoreError> {
        crate::secrets::delete(PROVIDER_NAMESPACE, alias).map_err(vault_error)
    }
}

// Any other non-macOS target (e.g. Linux) has no secure backend yet.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl SecretStore for KeychainSecretStore {
    fn load(&self, _alias: &str) -> Result<CodexCredential, SecretStoreError> {
        Err(SecretStoreError::Unavailable)
    }

    fn store(&self, _alias: &str, _credential: &CodexCredential) -> Result<(), SecretStoreError> {
        Err(SecretStoreError::Unavailable)
    }

    fn remove(&self, _alias: &str) -> Result<(), SecretStoreError> {
        Err(SecretStoreError::Unavailable)
    }
}

#[cfg(test)]
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

#[cfg(test)]
#[derive(Default)]
pub(crate) struct InMemorySecretStore {
    entries: Mutex<HashMap<String, CodexCredential>>,
    fail_stores: AtomicBool,
    fail_removes: AtomicBool,
}

#[cfg(test)]
impl InMemorySecretStore {
    pub(crate) fn fail_store(&self, failed: bool) {
        self.fail_stores.store(failed, Ordering::Relaxed);
    }

    pub(crate) fn fail_remove(&self, failed: bool) {
        self.fail_removes.store(failed, Ordering::Relaxed);
    }
}

#[cfg(test)]
impl SecretStore for InMemorySecretStore {
    fn load(&self, alias: &str) -> Result<CodexCredential, SecretStoreError> {
        self.entries
            .lock()
            .map_err(|_| SecretStoreError::OperationFailed)?
            .get(alias)
            .cloned()
            .ok_or(SecretStoreError::Missing)
    }

    fn store(&self, alias: &str, credential: &CodexCredential) -> Result<(), SecretStoreError> {
        if self.fail_stores.load(Ordering::Relaxed) {
            return Err(SecretStoreError::OperationFailed);
        }
        self.entries
            .lock()
            .map_err(|_| SecretStoreError::OperationFailed)?
            .insert(alias.to_owned(), credential.clone());
        Ok(())
    }

    fn remove(&self, alias: &str) -> Result<(), SecretStoreError> {
        if self.fail_removes.load(Ordering::Relaxed) {
            return Err(SecretStoreError::OperationFailed);
        }
        self.entries
            .lock()
            .map_err(|_| SecretStoreError::OperationFailed)?
            .remove(alias);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::{initialize_database, list_provider_accounts};
    use rusqlite::Connection;

    fn connection() -> Connection {
        let mut connection = Connection::open_in_memory().expect("in-memory SQLite");
        initialize_database(&mut connection).expect("database migration");
        connection
    }

    fn credential(account_id: &str) -> CodexCredential {
        CodexCredential::new(
            "access-token",
            "refresh-token",
            1_735_689_600_000,
            account_id,
            None,
            None,
        )
    }

    // The production credential path on Windows: serialize -> DPAPI vault ->
    // deserialize. InMemorySecretStore tests never exercise this seam, which is
    // exactly where the store returned Unavailable before the vault landed.
    #[cfg(target_os = "windows")]
    #[test]
    fn windows_provider_store_round_trips_a_real_credential_via_dpapi() {
        let store = KeychainSecretStore;
        // Unique per run so concurrent/parallel tests never collide on one key.
        let alias = format!("openai-codex-test-{}", crate::library::new_id().unwrap());
        let original = credential("account-dpapi");

        store.store(&alias, &original).expect("DPAPI store");
        let loaded = store.load(&alias).expect("DPAPI load");
        // CodexCredential has PartialEq but no Debug on purpose: a credential
        // must never be printable in a failure message.
        assert!(
            loaded == original,
            "DPAPI round-trip changed the credential"
        );

        // Overwrite proves replacement, not append, on the same key.
        let rotated = CodexCredential::new(
            "rotated-token",
            "rotated-refresh",
            1_800_000_000_000,
            "account-dpapi",
            Some("person@example.com".to_string()),
            None,
        );
        store.store(&alias, &rotated).expect("DPAPI overwrite");
        assert!(
            store.load(&alias).expect("reload") == rotated,
            "overwrite lost the rotated credential"
        );

        store.remove(&alias).expect("DPAPI remove");
        assert!(store.load(&alias).is_err());
        // Removing an absent key stays idempotent for disconnect.
        store.remove(&alias).expect("idempotent remove");
    }

    #[test]
    fn alias_validation_enforces_the_full_suffix_contract() {
        let max_suffix = "a".repeat(32);
        for suffix in ["1", "pessoal", "empresa", "a-b", max_suffix.as_str()] {
            validate_alias_suffix(suffix).expect("valid suffix");
            let alias = format!("{OPENAI_CODEX_ALIAS_PREFIX}{suffix}");
            assert_eq!(alias, format!("{OPENAI_CODEX_ALIAS_PREFIX}{suffix}"));
            validate_provider_alias(&alias).expect("valid full alias");
        }

        let too_long_suffix = "a".repeat(33);
        for suffix in ["", "A", "a b", "-a", "a-", "a--b", "a_b", "á"] {
            assert!(validate_alias_suffix(suffix).is_err());
        }
        assert!(validate_alias_suffix(&too_long_suffix).is_err());
        assert!(validate_provider_alias("openai-codex-pessoal/").is_err());
    }

    #[test]
    fn duplicate_alias_and_account_id_are_rejected_before_secret_storage() {
        let connection = connection();
        let secrets = InMemorySecretStore::default();
        commit_provider_account(
            &connection,
            &secrets,
            "openai-codex-one",
            &credential("account-one"),
        )
        .expect("account commit");

        assert!(matches!(
            commit_provider_account(
                &connection,
                &secrets,
                "openai-codex-one",
                &credential("account-two"),
            ),
            Err(ProviderAccountError::DuplicateAccount)
        ));
        assert!(matches!(
            commit_provider_account(
                &connection,
                &secrets,
                "openai-codex-two",
                &credential("account-one"),
            ),
            Err(ProviderAccountError::DuplicateAccount)
        ));
        assert!(matches!(
            secrets.load("openai-codex-two"),
            Err(SecretStoreError::Missing)
        ));
    }

    #[test]
    fn secret_store_failure_leaves_metadata_empty() {
        let connection = connection();
        let secrets = InMemorySecretStore::default();
        secrets.fail_store(true);

        let result = commit_provider_account(
            &connection,
            &secrets,
            "openai-codex-one",
            &credential("account-one"),
        );

        assert!(matches!(
            result,
            Err(ProviderAccountError::SecretStore(
                SecretStoreError::OperationFailed
            ))
        ));
        assert!(list_provider_accounts(&connection)
            .expect("account list")
            .is_empty());
    }

    #[test]
    fn database_failure_removes_the_new_secret() {
        let connection = connection();
        connection
            .execute_batch(
                "CREATE TRIGGER reject_provider_account_insert
                 BEFORE INSERT ON provider_accounts
                 BEGIN SELECT RAISE(ABORT, 'test failure'); END;",
            )
            .expect("failure trigger");
        let secrets = InMemorySecretStore::default();

        let result = commit_provider_account(
            &connection,
            &secrets,
            "openai-codex-one",
            &credential("account-one"),
        );

        assert!(matches!(result, Err(ProviderAccountError::Database)));
        assert!(matches!(
            secrets.load("openai-codex-one"),
            Err(SecretStoreError::Missing)
        ));
    }

    #[test]
    fn disconnect_removes_secret_before_metadata_and_is_idempotent_when_missing() {
        let connection = connection();
        let secrets = InMemorySecretStore::default();
        commit_provider_account(
            &connection,
            &secrets,
            "openai-codex-one",
            &credential("account-one"),
        )
        .expect("account commit");

        disconnect_provider_account(&connection, &secrets, "openai-codex-one").expect("disconnect");
        assert!(list_provider_accounts(&connection)
            .expect("account list")
            .is_empty());
        assert!(matches!(
            secrets.load("openai-codex-one"),
            Err(SecretStoreError::Missing)
        ));

        disconnect_provider_account(&connection, &secrets, "openai-codex-one")
            .expect("idempotent disconnect");
    }

    #[test]
    fn failed_secret_removal_keeps_metadata() {
        let connection = connection();
        let secrets = InMemorySecretStore::default();
        commit_provider_account(
            &connection,
            &secrets,
            "openai-codex-one",
            &credential("account-one"),
        )
        .expect("account commit");
        secrets.fail_remove(true);

        assert!(matches!(
            disconnect_provider_account(&connection, &secrets, "openai-codex-one"),
            Err(ProviderAccountError::SecretStore(
                SecretStoreError::OperationFailed
            ))
        ));
        assert_eq!(
            list_provider_accounts(&connection)
                .expect("account list")
                .len(),
            1
        );
    }

    #[test]
    fn provider_metadata_does_not_serialize_account_id() {
        let account = ProviderAccount {
            alias: "openai-codex-one".to_owned(),
            enabled: true,
            show_usage: true,
            show_third_party_usage: false,
            provider_kind: "openai-codex".to_owned(),
            created_at: 1_735_689_600,
            email: Some("person@example.com".to_owned()),
            account_type: ProviderAccountType::Personal,
            models: Vec::new(),
            models_available: false,
            custom: None,
        };
        let value = serde_json::to_value(account).expect("metadata JSON");
        assert!(value.get("accountId").is_none());
        assert_eq!(value["providerKind"], "openai-codex");
        assert_eq!(value["createdAt"], 1_735_689_600);
    }

    #[test]
    fn token_profile_extracts_subscription_metadata() {
        use base64::Engine;
        let payload = serde_json::json!({
            OPENAI_CODEX_AUTH_CLAIM: {
                "chatgpt_account_id": "ACCOUNT-ONE",
                "chatgpt_plan_type": "team"
            },
            OPENAI_CODEX_PROFILE_CLAIM: { "email": " Person@Example.COM " }
        });
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).expect("JWT payload"));
        let profile = token_profile(&format!("header.{payload}.signature"), None);

        assert_eq!(profile.account_id.as_deref(), Some("ACCOUNT-ONE"));
        assert_eq!(profile.email.as_deref(), Some("person@example.com"));
        assert_eq!(profile.plan_type.as_deref(), Some("team"));
        assert_eq!(
            classify_account_type(profile.plan_type.as_deref().expect("plan")),
            ProviderAccountType::Enterprise
        );
    }

    #[test]
    fn codex_model_list_is_filtered_sorted_and_normalized() {
        let payload = serde_json::json!({
            "models": [
                {
                    "slug": "hidden-model",
                    "display_name": "Hidden",
                    "visibility": "hidden",
                    "priority": 0
                },
                {
                    "slug": "gpt-later",
                    "display_name": "GPT Later",
                    "default_reasoning_level": "none",
                    "priority": 20
                },
                {
                    "id": "gpt-first",
                    "display_name": "GPT First",
                    "supported_reasoning_levels": [{ "effort": "medium" }],
                    "priority": 10
                }
            ]
        });

        assert_eq!(
            normalize_codex_models(&payload),
            Some(vec![
                ProviderModel {
                    id: "gpt-first".to_owned(),
                    name: "GPT First".to_owned(),
                    context_window: None,
                    reasoning_levels: vec!["medium".to_owned()],
                    default_reasoning_level: None,
                },
                ProviderModel {
                    id: "gpt-later".to_owned(),
                    name: "GPT Later".to_owned(),
                    context_window: None,
                    reasoning_levels: vec!["none".to_owned()],
                    default_reasoning_level: Some("none".to_owned()),
                },
            ])
        );
    }

    #[test]
    fn model_ipc_preserves_reported_reasoning_levels_and_default() {
        let payload = serde_json::json!({ "models": [{
            "id": "model-one",
            "supported_reasoning_levels": ["low", { "effort": "medium" }, "xhigh", "medium", "future"],
            "default_reasoning_level": "medium"
        }] });
        let models = normalize_codex_models(&payload).expect("model catalog");
        assert_eq!(
            serde_json::to_value(models).expect("IPC JSON"),
            serde_json::json!([{
                "id": "model-one", "name": "model-one",
                "reasoningLevels": ["low", "medium", "xhigh", "future"],
                "defaultReasoningLevel": "medium", "contextWindow": null
            }])
        );
    }

    #[test]
    fn context_window_uses_only_a_positive_reported_integer() {
        for (value, expected) in [
            (serde_json::json!(128000), Some(128000)),
            (serde_json::json!(0), None),
            (serde_json::json!(-1), None),
            (serde_json::json!(128.5), None),
            (serde_json::json!("128000"), None),
            (serde_json::Value::Null, None),
        ] {
            let models = normalize_codex_models(&serde_json::json!({"models":[{
                "id":"model", "context_window":value
            }]}))
            .unwrap();
            assert_eq!(models[0].context_window, expected);
            assert_eq!(
                serde_json::to_value(&models[0]).unwrap()["contextWindow"],
                serde_json::json!(expected)
            );
        }
    }

    #[test]
    fn reasoning_default_does_not_add_an_option_missing_from_an_explicit_list() {
        for levels in [
            serde_json::json!(["low", "medium"]),
            serde_json::json!([]),
            serde_json::json!(false),
        ] {
            let payload = serde_json::json!({ "models": [{
                "id": "model-one", "supported_reasoning_levels": levels,
                "default_reasoning_level": "high"
            }] });
            let models = normalize_codex_models(&payload).expect("model catalog");
            assert_eq!(models[0].default_reasoning_level, None);
            assert!(!models[0]
                .reasoning_levels
                .iter()
                .any(|level| level == "high"));
        }
    }

    #[test]
    fn reasoning_uses_only_reported_default_when_the_list_is_absent() {
        let payload = serde_json::json!({ "models": [
            { "id": "model-one", "default_reasoning_level": "minimal" },
            { "id": "model-two" }
        ] });
        let models = normalize_codex_models(&payload).expect("model catalog");
        assert_eq!(models[0].reasoning_levels, ["minimal"]);
        assert_eq!(
            models[0].default_reasoning_level.as_deref(),
            Some("minimal")
        );
        assert!(models[1].reasoning_levels.is_empty());
        assert_eq!(models[1].default_reasoning_level, None);
    }

    #[test]
    fn reasoning_filters_malformed_entries_and_retains_explicit_none() {
        let payload = serde_json::json!({ "models": [{
            "id": "model-one",
            "supported_reasoning_levels": [null, 1, {}, { "effort": false }, "", "<html>", "a".repeat(33),
                { "effort": " none " }, "minimal", "max", "ultra"],
            "default_reasoning_level": "none"
        }] });
        let models = normalize_codex_models(&payload).expect("model catalog");
        assert_eq!(
            models[0].reasoning_levels,
            ["none", "minimal", "max", "ultra"]
        );
        assert_eq!(models[0].default_reasoning_level.as_deref(), Some("none"));
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderError {
    pub code: String,
    pub message: String,
}
impl From<PersistenceError> for ProviderError {
    fn from(_: PersistenceError) -> Self {
        Self::database()
    }
}

impl ProviderError {
    pub(crate) fn new(code: &'static str, message: &'static str) -> Self {
        Self {
            code: code.to_owned(),
            message: message.to_owned(),
        }
    }

    fn invalid_alias() -> Self {
        Self::new(
            "invalid_alias",
            "O alias do provedor não segue o formato permitido.",
        )
    }

    fn internal() -> Self {
        Self::new(
            "internal_error",
            "Não foi possível concluir a conexão do provedor.",
        )
    }

    fn database() -> Self {
        Self::new("database", "Não foi possível acessar as contas conectadas.")
    }

    fn from_account_error(error: ProviderAccountError) -> Self {
        match error {
            ProviderAccountError::InvalidAlias(_) => Self::invalid_alias(),
            ProviderAccountError::InvalidAccountId => Self::new(
                "malformed_token",
                "A conta do provedor não foi identificada.",
            ),
            ProviderAccountError::DuplicateAccount => {
                Self::new("duplicate_account", "Esta conta já está conectada.")
            }
            #[cfg(not(target_os = "macos"))]
            ProviderAccountError::SecretStore(SecretStoreError::Unavailable) => Self::new(
                "secret_store",
                "O armazenamento seguro não está disponível neste sistema.",
            ),
            #[cfg(any(target_os = "macos", target_os = "windows", test))]
            ProviderAccountError::SecretStore(_) => Self::new(
                "secret_store",
                "Não foi possível salvar a credencial com segurança.",
            ),
            ProviderAccountError::Database => Self::database(),
            ProviderAccountError::SecretCleanup => Self::new(
                "secret_store",
                "Não foi possível desfazer o armazenamento seguro após uma falha.",
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenAiCodexConnectionStart {
    #[serde(rename = "flowId")]
    pub flow_id: String,
    #[serde(rename = "authorizationUrl")]
    pub authorization_url: String,
}

pub(crate) const OPENAI_CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub(crate) const OPENAI_CODEX_AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
pub(crate) const OPENAI_CODEX_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
pub(crate) const OPENAI_CODEX_SCOPE: &str = "openid profile email offline_access";
const OPENAI_CODEX_AUTH_CLAIM: &str = "https://api.openai.com/auth";
const OPENAI_CODEX_CALLBACK_ROUTE: &str = "/auth/callback";
const OPENAI_CODEX_CALLBACK_PORTS: [u16; 2] = [1455, 1457];
const OPENAI_CODEX_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10 * 60);

#[derive(Clone)]
struct OAuthEndpoints {
    authorize_url: String,
    token_url: String,
    allow_http: bool,
}

impl OAuthEndpoints {
    fn production() -> Self {
        Self {
            authorize_url: OPENAI_CODEX_AUTHORIZE_URL.to_owned(),
            token_url: OPENAI_CODEX_TOKEN_URL.to_owned(),
            allow_http: false,
        }
    }

    #[cfg(test)]
    fn test(authorize_url: impl Into<String>, token_url: impl Into<String>) -> Self {
        Self {
            authorize_url: authorize_url.into(),
            token_url: token_url.into(),
            allow_http: true,
        }
    }
}

struct OAuthFlow {
    id: String,
    alias: String,
    replacing: Option<ProviderAccountRecord>,
    state: String,
    verifier: String,
    redirect_uri: String,
    cancelled: std::sync::atomic::AtomicBool,
    wait_started: std::sync::atomic::AtomicBool,
    commit_guard: std::sync::Mutex<()>,
    result: std::sync::Mutex<Option<Result<ProviderAccount, ProviderError>>>,
    result_ready: std::sync::Condvar,
}

impl OAuthFlow {
    fn complete(&self, result: Result<ProviderAccount, ProviderError>) {
        let Ok(mut stored) = self.result.lock() else {
            return;
        };
        if stored.is_none() {
            *stored = Some(result);
            self.result_ready.notify_all();
        }
    }

    fn wait(&self) -> Result<ProviderAccount, ProviderError> {
        let mut stored = self.result.lock().map_err(|_| ProviderError::internal())?;
        while stored.is_none() {
            stored = self
                .result_ready
                .wait(stored)
                .map_err(|_| ProviderError::internal())?;
        }
        stored.clone().ok_or_else(ProviderError::internal)?
    }

    fn is_pending(&self) -> bool {
        self.result
            .lock()
            .map(|stored| stored.is_none())
            .unwrap_or(false)
    }
}

struct OAuthManager {
    usage_cache: usage::UsageCache,
    credentials_guard: std::sync::Mutex<()>,
    active_flow: std::sync::Mutex<Option<String>>,
    flows: std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<OAuthFlow>>>,
    endpoints: OAuthEndpoints,
    callback_ports: Vec<u16>,
    timeout: std::time::Duration,
    secret_store: std::sync::Arc<dyn SecretStore>,
}

impl OAuthManager {
    fn new(
        endpoints: OAuthEndpoints,
        callback_ports: Vec<u16>,
        timeout: std::time::Duration,
        secret_store: std::sync::Arc<dyn SecretStore>,
    ) -> Self {
        Self {
            credentials_guard: std::sync::Mutex::new(()),
            usage_cache: Default::default(),
            active_flow: std::sync::Mutex::new(None),
            flows: std::sync::Mutex::new(std::collections::HashMap::new()),
            endpoints,
            callback_ports,
            timeout,
            secret_store,
        }
    }

    fn production(secret_store: std::sync::Arc<dyn SecretStore>) -> Self {
        Self::new(
            OAuthEndpoints::production(),
            OPENAI_CODEX_CALLBACK_PORTS.to_vec(),
            OPENAI_CODEX_TIMEOUT,
            secret_store,
        )
    }

    fn list_accounts(
        &self,
        app_state: &persistence::AppState,
        home_dir: &std::path::Path,
        client: Option<&reqwest::blocking::Client>,
    ) -> Result<Vec<ProviderAccount>, ProviderError> {
        // Refresh can rotate credentials: serialize it with connection and disconnection.
        let _guard = self
            .credentials_guard
            .lock()
            .map_err(|_| ProviderError::internal())?;
        let records = app_state
            .list_provider_accounts(home_dir)
            .map_err(|_| ProviderError::database())?;
        records
            .into_iter()
            .map(|record| {
                if record.provider_kind == "custom" {
                    return custom::account(app_state, home_dir, record);
                }
                Ok(account_details(
                    record,
                    self.secret_store.as_ref(),
                    &self.endpoints,
                    client,
                ))
            })
            .collect()
    }

    fn disconnect_account(
        &self,
        app_state: &persistence::AppState,
        home_dir: &std::path::Path,
        alias: &str,
    ) -> Result<(), ProviderError> {
        let _guard = self
            .credentials_guard
            .lock()
            .map_err(|_| ProviderError::internal())?;
        disconnect_provider_account_with_state(
            app_state,
            home_dir,
            self.secret_store.as_ref(),
            alias,
        )
    }

    fn begin(
        self: &std::sync::Arc<Self>,
        app_state: &persistence::AppState,
        home_dir: &std::path::Path,
        alias: &str,
    ) -> Result<OpenAiCodexConnectionStart, ProviderError> {
        self.begin_connection(app_state, home_dir, alias, false)
    }

    fn begin_connection(
        self: &std::sync::Arc<Self>,
        app_state: &persistence::AppState,
        home_dir: &std::path::Path,
        alias: &str,
        reauthorize: bool,
    ) -> Result<OpenAiCodexConnectionStart, ProviderError> {
        validate_provider_alias(alias).map_err(|_| ProviderError::invalid_alias())?;

        let mut active = self
            .active_flow
            .lock()
            .map_err(|_| ProviderError::internal())?;
        if active.is_some() {
            return Err(ProviderError::new(
                "active_flow",
                "Já existe uma conexão em andamento.",
            ));
        }

        let existing = app_state
            .list_provider_accounts(home_dir)
            .map_err(|_| ProviderError::database())?;
        let replacing = existing.into_iter().find(|account| account.alias == alias);
        if reauthorize && replacing.is_none() {
            return Err(reauthorization::account_changed());
        }
        if !reauthorize && replacing.is_some() {
            return Err(ProviderError::new(
                "duplicate_account",
                "Este alias já está conectado.",
            ));
        }

        let google = alias.starts_with("antigravity-");
        let (listener, port) = bind_callback_listener(if google {
            &[51121]
        } else {
            &self.callback_ports
        })?;
        listener.set_nonblocking(true).map_err(|_| {
            ProviderError::new(
                "port_unavailable",
                "As portas de retorno não estão disponíveis.",
            )
        })?;

        let (verifier, challenge) = create_pkce()?;
        let state = random_token(32)?;
        let flow_id = random_token(16)?;
        let redirect_uri = if google {
            format!("http://127.0.0.1:{port}/oauth-callback")
        } else {
            format!("http://localhost:{port}{OPENAI_CODEX_CALLBACK_ROUTE}")
        };
        let authorization_url = if google {
            antigravity::authorization_url(&redirect_uri, &challenge, &state)?
        } else {
            build_authorization_url(
                &self.endpoints.authorize_url,
                &redirect_uri,
                &challenge,
                &state,
            )?
        };

        let flow = std::sync::Arc::new(OAuthFlow {
            id: flow_id.clone(),
            alias: alias.to_owned(),
            replacing,
            state,
            verifier,
            redirect_uri,
            cancelled: std::sync::atomic::AtomicBool::new(false),
            wait_started: std::sync::atomic::AtomicBool::new(false),
            commit_guard: std::sync::Mutex::new(()),
            result: std::sync::Mutex::new(None),
            result_ready: std::sync::Condvar::new(),
        });

        self.flows
            .lock()
            .map_err(|_| ProviderError::internal())?
            .insert(flow_id.clone(), flow.clone());
        *active = Some(flow_id.clone());
        drop(active);

        let manager = self.clone();
        let app_state = app_state.clone();
        let home_dir = home_dir.to_path_buf();
        if std::thread::Builder::new()
            .name("jarvis-openai-codex-oauth".to_owned())
            .spawn(move || run_oauth_flow(manager, flow, listener, app_state, home_dir))
            .is_err()
        {
            self.clear_active(&flow_id);
            if let Ok(mut flows) = self.flows.lock() {
                flows.remove(&flow_id);
            }
            return Err(ProviderError::internal());
        }

        Ok(OpenAiCodexConnectionStart {
            flow_id,
            authorization_url,
        })
    }

    fn flow(&self, flow_id: &str) -> Result<std::sync::Arc<OAuthFlow>, ProviderError> {
        self.flows
            .lock()
            .map_err(|_| ProviderError::internal())?
            .get(flow_id)
            .cloned()
            .ok_or_else(|| ProviderError::new("flow_not_found", "A conexão solicitada não existe."))
    }

    fn wait(&self, flow_id: &str) -> Result<ProviderAccount, ProviderError> {
        let flow = self.flow(flow_id)?;
        flow.wait_started
            .store(true, std::sync::atomic::Ordering::Release);
        let result = flow.wait();
        if let Ok(mut flows) = self.flows.lock() {
            flows.remove(flow_id);
        }
        result
    }

    fn cancel(&self, flow_id: &str) -> Result<(), ProviderError> {
        let flow = self
            .flows
            .lock()
            .map_err(|_| ProviderError::internal())?
            .get(flow_id)
            .cloned();
        let Some(flow) = flow else {
            return Ok(());
        };
        if flow.is_pending() {
            flow.cancelled
                .store(true, std::sync::atomic::Ordering::Release);
            let _commit_guard = flow
                .commit_guard
                .lock()
                .map_err(|_| ProviderError::internal())?;
            if flow.is_pending() {
                self.clear_active(flow_id);
                flow.complete(Err(ProviderError::new(
                    "cancelled",
                    "A conexão foi cancelada.",
                )));
            }
        }
        self.flows
            .lock()
            .map_err(|_| ProviderError::internal())?
            .remove(flow_id);
        Ok(())
    }

    fn clear_active(&self, flow_id: &str) {
        let Ok(mut active) = self.active_flow.lock() else {
            return;
        };
        if active.as_deref() == Some(flow_id) {
            *active = None;
        }
    }
}

#[derive(Clone)]
pub(crate) struct OpenAiCodexState {
    manager: std::sync::Arc<OAuthManager>,
}

impl Default for OpenAiCodexState {
    fn default() -> Self {
        Self {
            manager: std::sync::Arc::new(OAuthManager::production(std::sync::Arc::new(
                KeychainSecretStore,
            ))),
        }
    }
}

impl OpenAiCodexState {
    /// Resolve inference credentials under the same lock as refresh/disconnect.
    /// The returned secret never crosses IPC or enters the session journal.
    pub(crate) fn inference_credential(
        &self,
        state: &persistence::AppState,
        home: &std::path::Path,
        alias: &str,
        model: &str,
        reasoning: Option<&str>,
    ) -> Result<CodexCredential, ProviderError> {
        self.inference_model(state, home, alias, model, reasoning)
            .map(|(credential, _)| credential)
    }

    pub(crate) fn inference_model(
        &self,
        state: &persistence::AppState,
        home: &std::path::Path,
        alias: &str,
        model: &str,
        reasoning: Option<&str>,
    ) -> Result<(CodexCredential, ProviderModel), ProviderError> {
        let (credential, models) = self.credential_and_models(state, home, alias)?;
        let selected = models.iter().find(|item| item.id == model).ok_or_else(|| {
            ProviderError::new(
                "invalid_model",
                "O modelo selecionado não está disponível nesta conta.",
            )
        })?;
        if reasoning.is_some_and(|effort| {
            !selected
                .reasoning_levels
                .iter()
                .any(|level| level == effort)
        }) {
            return Err(ProviderError::new(
                "invalid_reasoning",
                "O nível de raciocínio não é aceito pelo modelo selecionado.",
            ));
        }
        Ok((credential, selected.clone()))
    }

    pub(crate) fn credential_and_models(
        &self,
        state: &persistence::AppState,
        home: &std::path::Path,
        alias: &str,
    ) -> Result<(CodexCredential, Vec<ProviderModel>), ProviderError> {
        custom::validate_alias(alias)?;
        let _guard = self
            .manager
            .credentials_guard
            .lock()
            .map_err(|_| ProviderError::internal())?;
        let records = state
            .list_provider_accounts(home)
            .map_err(|_| ProviderError::database())?;
        let record = records
            .iter()
            .find(|record| record.alias == alias)
            .ok_or_else(|| {
                ProviderError::new("account_missing", "A conta selecionada foi desconectada.")
            })?;
        if !record.enabled {
            return Err(ProviderError::new(
                "account_disabled",
                "Ative a conta nas configurações para usá-la.",
            ));
        }
        let mut credential = self.manager.secret_store.load(alias).map_err(|_| {
            ProviderError::new(
                "credential_missing",
                if record.provider_kind == "custom" {
                    "Edite o provedor Custom e informe sua chave de API."
                } else {
                    "Reconecte a conta nas configurações para enviar mensagens."
                },
            )
        })?;
        if record.provider_kind == "custom" {
            if credential.account_id != record.account_id || credential.project_id.is_some() {
                return Err(ProviderError::new(
                    "account_mismatch",
                    "Revise a chave do provedor Custom.",
                ));
            }
            let config = custom::load(state, home, alias)?;
            let models = config.catalog();
            credential.custom = Some(config);
            return Ok((credential, models));
        }
        if credential.account_id != record.account_id
            || (record.provider_kind == "antigravity") != credential.project_id.is_some()
        {
            return Err(ProviderError::new(
                "account_mismatch",
                "Reconecte a conta selecionada nas configurações.",
            ));
        }
        if credential.expires <= current_time_millis()? + 60_000 {
            credential = refresh_credential(&self.manager.endpoints, &credential)?;
            self.manager
                .secret_store
                .store(alias, &credential)
                .map_err(|_| ProviderError::internal())?;
        }
        let client = build_codex_client().map_err(|_| ProviderError::internal())?;
        let models = fetch_provider_models(&client, &mut credential).ok_or_else(|| {
            ProviderError::new(
                "catalog_unavailable",
                "Não foi possível verificar os modelos da conta. Tente novamente.",
            )
        })?;
        Ok((credential, models))
    }
}

fn random_token(length: usize) -> Result<String, ProviderError> {
    let mut bytes = vec![0_u8; length];
    getrandom::fill(&mut bytes).map_err(|_| ProviderError::internal())?;
    use base64::Engine;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

fn create_pkce() -> Result<(String, String), ProviderError> {
    let mut verifier_bytes = [0_u8; 64];
    getrandom::fill(&mut verifier_bytes).map_err(|_| ProviderError::internal())?;
    use base64::Engine;
    let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(verifier_bytes);
    use sha2::{Digest, Sha256};
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(verifier.as_bytes()));
    Ok((verifier, challenge))
}

fn bind_callback_listener(ports: &[u16]) -> Result<(std::net::TcpListener, u16), ProviderError> {
    for port in ports {
        match std::net::TcpListener::bind(("127.0.0.1", *port)) {
            Ok(listener) => return Ok((listener, *port)),
            Err(_) => continue,
        }
    }
    Err(ProviderError::new(
        "port_unavailable",
        "As portas de retorno não estão disponíveis.",
    ))
}

fn build_authorization_url(
    authorize_url: &str,
    redirect_uri: &str,
    challenge: &str,
    state: &str,
) -> Result<String, ProviderError> {
    let mut url = url::Url::parse(authorize_url).map_err(|_| ProviderError::internal())?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("response_type", "code")
            .append_pair("client_id", OPENAI_CODEX_CLIENT_ID)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("scope", OPENAI_CODEX_SCOPE)
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", state)
            .append_pair("id_token_add_organizations", "true")
            .append_pair("codex_cli_simplified_flow", "true")
            .append_pair("originator", "jarvis");
    }
    Ok(url.to_string())
}

fn run_oauth_flow(
    manager: std::sync::Arc<OAuthManager>,
    flow: std::sync::Arc<OAuthFlow>,
    listener: std::net::TcpListener,
    app_state: persistence::AppState,
    home_dir: std::path::PathBuf,
) {
    let result = run_oauth_flow_inner(&manager, &flow, listener, &app_state, &home_dir);
    manager.clear_active(&flow.id);
    flow.complete(result);
}

fn run_oauth_flow_inner(
    manager: &OAuthManager,
    flow: &OAuthFlow,
    listener: std::net::TcpListener,
    app_state: &persistence::AppState,
    home_dir: &std::path::Path,
) -> Result<ProviderAccount, ProviderError> {
    let google = flow.alias.starts_with("antigravity-");
    let code = wait_for_callback_route(
        listener,
        &flow.state,
        &flow.cancelled,
        manager.timeout,
        if google {
            "/oauth-callback"
        } else {
            OPENAI_CODEX_CALLBACK_ROUTE
        },
    )?;
    if flow.cancelled.load(std::sync::atomic::Ordering::Acquire) {
        return Err(ProviderError::new("cancelled", "A conexão foi cancelada."));
    }

    let credential = if google {
        antigravity::exchange(&code, &flow.verifier, &flow.redirect_uri, &flow.cancelled)?
    } else {
        let token = exchange_authorization_code(
            &manager.endpoints,
            &code,
            &flow.verifier,
            &flow.redirect_uri,
        )?;
        if flow.cancelled.load(std::sync::atomic::Ordering::Acquire) {
            return Err(ProviderError::new("cancelled", "A conexão foi cancelada."));
        }
        CodexCredential::new(
            token.access,
            token.refresh,
            token.expires,
            token.account_id,
            token.email,
            token.plan_type,
        )
    };
    let _commit_guard = flow
        .commit_guard
        .lock()
        .map_err(|_| ProviderError::internal())?;
    if flow.cancelled.load(std::sync::atomic::Ordering::Acquire) {
        let result = Err(ProviderError::new("cancelled", "A conexão foi cancelada."));
        flow.complete(result.clone());
        return result;
    }
    let _credentials_guard = manager
        .credentials_guard
        .lock()
        .map_err(|_| ProviderError::internal())?;
    let result = if let Some(expected) = &flow.replacing {
        app_state.with_connection(home_dir, |connection| {
            reauthorization::replace_account(
                connection,
                manager.secret_store.as_ref(),
                expected,
                &credential,
            )
        })
    } else {
        commit_provider_account_with_state(
            app_state,
            home_dir,
            manager.secret_store.as_ref(),
            &flow.alias,
            &credential,
        )
    };
    if result.is_ok() {
        manager.usage_cache.invalidate(&flow.alias);
    }
    flow.complete(result.clone());
    result
}

enum CallbackEvent {
    Code(String),
    Denied,
    StateMismatch,
    Invalid,
    Continue,
}

fn wait_for_callback_route(
    listener: std::net::TcpListener,
    expected_state: &str,
    cancelled: &std::sync::atomic::AtomicBool,
    timeout: std::time::Duration,
    route: &str,
) -> Result<String, ProviderError> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if cancelled.load(std::sync::atomic::Ordering::Acquire) {
            return Err(ProviderError::new("cancelled", "A conexão foi cancelada."));
        }
        if std::time::Instant::now() >= deadline {
            return Err(ProviderError::new(
                "timeout",
                "A conexão expirou antes da autenticação.",
            ));
        }

        match listener.accept() {
            Ok((mut stream, _)) => match process_callback_route(&mut stream, expected_state, route)
            {
                CallbackEvent::Code(code) => return Ok(code),
                CallbackEvent::Denied => {
                    return Err(ProviderError::new("denied", "A autorização foi recusada."));
                }
                CallbackEvent::StateMismatch => {
                    return Err(ProviderError::new(
                        "callback_state",
                        "A validação da autenticação falhou.",
                    ));
                }
                CallbackEvent::Invalid | CallbackEvent::Continue => continue,
            },
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(_) => {
                return Err(ProviderError::new(
                    "callback_error",
                    "Não foi possível receber o retorno da autenticação.",
                ));
            }
        }
    }
}

fn process_callback_route(
    stream: &mut std::net::TcpStream,
    expected_state: &str,
    route: &str,
) -> CallbackEvent {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(250)));
    let request = match read_http_request(stream) {
        Ok(request) => request,
        Err(()) => {
            write_html_response(stream, "400 Bad Request", "Não foi possível ler o retorno.");
            return CallbackEvent::Continue;
        }
    };

    let Some(request_line) = request.lines().next() else {
        write_html_response(stream, "400 Bad Request", "Retorno inválido.");
        return CallbackEvent::Continue;
    };
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    if method != "GET" || !target.starts_with('/') {
        write_html_response(stream, "404 Not Found", "Rota não encontrada.");
        return CallbackEvent::Continue;
    }

    let Ok(url) = url::Url::parse(&format!("http://localhost{target}")) else {
        write_html_response(stream, "400 Bad Request", "Retorno inválido.");
        return CallbackEvent::Continue;
    };
    if url.path() != route {
        write_html_response(stream, "404 Not Found", "Rota não encontrada.");
        return CallbackEvent::Continue;
    }

    let states = url
        .query_pairs()
        .filter(|(key, _)| key == "state")
        .map(|(_, value)| value.into_owned())
        .collect::<Vec<_>>();
    if states.len() != 1 || states[0] != expected_state {
        write_html_response(
            stream,
            "400 Bad Request",
            "A validação da autenticação falhou.",
        );
        return CallbackEvent::StateMismatch;
    }

    if url
        .query_pairs()
        .any(|(key, value)| key == "error" && !value.is_empty())
    {
        write_html_response(stream, "400 Bad Request", "A autorização foi recusada.");
        return CallbackEvent::Denied;
    }

    let code = url
        .query_pairs()
        .find(|(key, value)| key == "code" && !value.is_empty())
        .map(|(_, value)| value.into_owned());
    let Some(code) = code else {
        write_html_response(
            stream,
            "400 Bad Request",
            "O retorno não contém um código válido.",
        );
        return CallbackEvent::Invalid;
    };

    write_html_response(
        stream,
        "200 OK",
        "Autenticação recebida. Você pode fechar esta janela.",
    );
    CallbackEvent::Code(code)
}

fn read_http_request(stream: &mut std::net::TcpStream) -> Result<String, ()> {
    use std::io::Read;
    let mut bytes = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    while bytes.len() < 16 * 1024 {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                bytes.extend_from_slice(&chunk[..read]);
                if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
                ) =>
            {
                continue
            }
            Err(_) => return Err(()),
        }
    }
    if bytes.len() >= 16 * 1024 || !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
        return Err(());
    }
    String::from_utf8(bytes).map_err(|_| ())
}

/// The current Jarvis mark, embedded once as a data URI. The callback page is
/// served by a throwaway local listener with no static assets, so the logo must
/// travel inline. The raw PNG ships in the binary, not a base64 source literal,
/// and the encoding runs at most once per process.
fn embedded_logo_data_uri() -> &'static str {
    use base64::Engine;
    use std::sync::LazyLock;
    static LOGO: LazyLock<String> = LazyLock::new(|| {
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD
                .encode(include_bytes!("../icons/128x128.png"))
        )
    });
    &LOGO
}

fn render_callback_html(status: &str, body: &str) -> String {
    let is_success = status.starts_with("200");
    let (accent_bar, badge_class, badge_icon, badge_text, heading, description, info_text) =
        if is_success {
            (
                "linear-gradient(90deg, #56b6c2, #61afef, #98c379)",
                "badge-success",
                concat!(
                    "<svg viewBox=\"0 0 16 16\" width=\"14\" height=\"14\" fill=\"none\" ",
                    "stroke=\"#98c379\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\">",
                    "<circle cx=\"8\" cy=\"8\" r=\"7\"/><path d=\"m5 8 2 2 4-4\"/></svg>"
                ),
                "Autenticação recebida",
                "Conexão autorizada",
                "Volte ao Jarvis para concluir a conexão da conta.",
                "Você pode fechar esta aba com segurança e voltar ao aplicativo.",
            )
        } else {
            (
                "linear-gradient(90deg, #e06c75, #e5c07b)",
                "badge-error",
                concat!(
                    "<svg viewBox=\"0 0 16 16\" width=\"14\" height=\"14\" fill=\"none\" ",
                    "stroke=\"#e06c75\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\">",
                    "<circle cx=\"8\" cy=\"8\" r=\"7\"/><path d=\"M8 5v4M8 11.5h.01\"/></svg>"
                ),
                "Falha na requisição",
                "Não foi possível autenticar",
                body,
                "Retorne ao Jarvis para tentar iniciar uma nova conexão.",
            )
        };

    let logo = embedded_logo_data_uri();
    let close_script = include_str!("../../src/components/settings/authorization-callback.ts");
    format!(
        r##"<!DOCTYPE html>
<html lang="pt-BR">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Jarvis — Autenticação</title>
  <style>
    * {{ box-sizing: border-box; margin: 0; padding: 0; }}
    body {{
      background-color: #282c34;
      background-image: radial-gradient(ellipse 80% 50% at 50% -20%, rgba(97, 175, 239, 0.12), rgba(40, 44, 52, 0));
      color: #abb2bf;
      font-family: 'Roboto', -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
      display: flex;
      align-items: center;
      justify-content: center;
      min-height: 100vh;
      padding: 24px;
      -webkit-font-smoothing: antialiased;
    }}
    .card {{
      background: #21252b;
      border: 1px solid #3e4451;
      border-radius: 16px;
      padding: 40px 32px;
      max-width: 440px;
      width: 100%;
      text-align: center;
      display: flex;
      flex-direction: column;
      align-items: center;
      box-shadow: 0 24px 48px -12px rgba(0, 0, 0, 0.55), 0 0 0 1px rgba(62, 68, 81, 0.4);
      position: relative;
      overflow: hidden;
    }}
    .accent-bar {{
      position: absolute;
      top: 0;
      left: 0;
      right: 0;
      height: 3px;
      background: {accent_bar};
    }}
    .logo-container {{
      display: flex;
      align-items: center;
      justify-content: center;
      width: 72px;
      height: 72px;
      border-radius: 18px;
      background: #1e2227;
      border: 1px solid #3e4451;
      margin-bottom: 20px;
      flex-shrink: 0;
      box-shadow: 0 8px 16px -4px rgba(0, 0, 0, 0.35);
    }}
    .status-badge {{
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 4px 14px;
      border-radius: 9999px;
      font-size: 12px;
      font-weight: 500;
      margin-bottom: 16px;
    }}
    .badge-success {{
      background: rgba(152, 195, 121, 0.12);
      color: #98c379;
      border: 1px solid rgba(152, 195, 121, 0.3);
    }}
    .badge-error {{
      background: rgba(224, 108, 117, 0.12);
      color: #e06c75;
      border: 1px solid rgba(224, 108, 117, 0.3);
    }}
    h1 {{
      font-size: 20px;
      font-weight: 600;
      color: #e6e6e6;
      margin-bottom: 8px;
      letter-spacing: -0.01em;
    }}
    .desc {{
      font-size: 14px;
      line-height: 1.5;
      color: #abb2bf;
      margin-bottom: 22px;
    }}
    .callout {{
      width: 100%;
      background: #1e2227;
      border: 1px solid rgba(62, 68, 81, 0.7);
      border-radius: 10px;
      padding: 12px 16px;
    }}
    .btn {{
      appearance: none;
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 8px;
      width: 100%;
      min-height: 40px;
      margin-top: 24px;
      padding: 10px 16px;
      border: 1px solid rgba(97, 175, 239, 0.35);
      border-radius: 8px;
      background: rgba(97, 175, 239, 0.12);
      color: #61afef;
      font: inherit;
      font-size: 14px;
      font-weight: 500;
      line-height: 1.4;
      cursor: pointer;
      transition: background-color 150ms, border-color 150ms, color 150ms;
    }}
    .btn:hover:not(:disabled) {{
      background: rgba(97, 175, 239, 0.25);
      border-color: rgba(97, 175, 239, 0.6);
      color: #ffffff;
    }}
    .btn:focus-visible {{
      outline: 2px solid #61afef;
      outline-offset: 3px;
    }}
    .btn:disabled {{
      cursor: wait;
      opacity: 0.7;
    }}
    .close-feedback {{
      margin-top: 16px;
      color: #abb2bf;
      font-size: 13px;
      line-height: 1.5;
      text-wrap: pretty;
    }}
    .close-feedback:focus {{ outline: none; }}
    @media (prefers-reduced-motion: reduce) {{
      .btn {{ transition: none; }}
    }}
  </style>
</head>
<body>
  <div class="card">
    <div class="accent-bar"></div>
    <div class="logo-container">
      <img src="{logo}" alt="Jarvis" width="52" height="52" style="display:block;border-radius:14px;" />
    </div>
    <div class="status-badge {badge_class}">
      {badge_icon}
      <span>{badge_text}</span>
    </div>
    <h1>{heading}</h1>
    <p class="desc">{description}</p>
    <div class="callout">{info_text}</div>
    <button class="btn" id="close-tab" type="button">
      <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="m6 6 12 12M18 6 6 18"/></svg>
      Fechar aba
    </button>
    <p class="close-feedback" id="close-feedback" role="status" tabindex="-1" hidden></p>
  </div>
  <script>{close_script}</script>
</body>
</html>"##
    )
}

fn write_html_response(stream: &mut std::net::TcpStream, status: &str, body: &str) {
    use std::io::Write;
    let html = render_callback_html(status, body);
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
        html.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

struct OAuthToken {
    access: String,
    refresh: String,
    expires: i64,
    account_id: String,
    email: Option<String>,
    plan_type: Option<String>,
}

#[derive(Default)]
struct TokenProfile {
    account_id: Option<String>,
    email: Option<String>,
    plan_type: Option<String>,
}

fn decode_jwt_payload(token: &str) -> Option<serde_json::Value> {
    let mut parts = token.split('.');
    let _header = parts.next()?;
    let payload = parts.next()?.trim_end_matches('=');
    let _signature = parts.next()?;
    if payload.is_empty() || parts.next().is_some() {
        return None;
    }

    use base64::Engine;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn bounded_claim(value: Option<&serde_json::Value>, max_length: usize) -> Option<String> {
    let value = value?.as_str()?.trim();
    if value.is_empty() || value.len() > max_length {
        return None;
    }
    Some(value.to_owned())
}

fn normalized_claim(value: Option<&serde_json::Value>, max_length: usize) -> Option<String> {
    bounded_claim(value, max_length).map(|value| value.to_lowercase())
}

fn token_profile(access_token: &str, id_token: Option<&str>) -> TokenProfile {
    let access = decode_jwt_payload(access_token);
    let identity = id_token.and_then(decode_jwt_payload);
    let access_auth = access
        .as_ref()
        .and_then(|value| value.get(OPENAI_CODEX_AUTH_CLAIM));
    let identity_auth = identity
        .as_ref()
        .and_then(|value| value.get(OPENAI_CODEX_AUTH_CLAIM));
    let access_profile = access
        .as_ref()
        .and_then(|value| value.get(OPENAI_CODEX_PROFILE_CLAIM));
    let identity_profile = identity
        .as_ref()
        .and_then(|value| value.get(OPENAI_CODEX_PROFILE_CLAIM));

    TokenProfile {
        account_id: bounded_claim(
            access_auth
                .and_then(|value| value.get("chatgpt_account_id"))
                .or_else(|| identity_auth.and_then(|value| value.get("chatgpt_account_id"))),
            256,
        ),
        email: normalized_claim(
            access_profile
                .and_then(|value| value.get("email"))
                .or_else(|| identity_profile.and_then(|value| value.get("email"))),
            320,
        ),
        plan_type: normalized_claim(
            access_auth
                .and_then(|value| value.get("chatgpt_plan_type"))
                .or_else(|| identity_auth.and_then(|value| value.get("chatgpt_plan_type"))),
            64,
        ),
    }
}

fn current_time_millis() -> Result<i64, ProviderError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| ProviderError::internal())?
        .as_millis();
    i64::try_from(now).map_err(|_| ProviderError::internal())
}

fn request_oauth_token(
    endpoints: &OAuthEndpoints,
    form: String,
    stored: Option<&CodexCredential>,
) -> Result<OAuthToken, ProviderError> {
    let endpoint = url::Url::parse(&endpoints.token_url).map_err(|_| {
        ProviderError::new("token_exchange", "O endpoint de autenticação é inválido.")
    })?;
    if !endpoints.allow_http && endpoint.scheme() != "https" {
        return Err(ProviderError::new(
            "token_exchange",
            "O endpoint de autenticação não usa uma conexão segura.",
        ));
    }

    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|_| {
            ProviderError::new(
                "token_exchange",
                "Não foi possível trocar o código de autenticação.",
            )
        })?;
    let response = client
        .post(endpoint)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form)
        .send()
        .map_err(|_| {
            ProviderError::new(
                "token_exchange",
                "Não foi possível trocar o código de autenticação.",
            )
        })?;
    if !response.status().is_success() {
        return Err(ProviderError::new(
            "token_exchange",
            "O servidor recusou a autenticação.",
        ));
    }
    let body = response.bytes().map_err(|_| {
        ProviderError::new("malformed_token", "A resposta de autenticação é inválida.")
    })?;
    let value: serde_json::Value = serde_json::from_slice(&body).map_err(|_| {
        ProviderError::new("malformed_token", "A resposta de autenticação é inválida.")
    })?;
    let access = value
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            ProviderError::new(
                "malformed_token",
                "A resposta de autenticação não contém os campos necessários.",
            )
        })?;
    let refresh = value
        .get("refresh_token")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| stored.map(|credential| credential.refresh.clone()))
        .ok_or_else(|| {
            ProviderError::new(
                "malformed_token",
                "A resposta de autenticação não contém os campos necessários.",
            )
        })?;
    let expires_in = value
        .get("expires_in")
        .and_then(serde_json::Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .ok_or_else(|| {
            ProviderError::new(
                "malformed_token",
                "A resposta de autenticação não contém os campos necessários.",
            )
        })?;
    let expires_delta = (expires_in * 1000.0).round();
    if expires_delta > i64::MAX as f64 {
        return Err(ProviderError::new(
            "malformed_token",
            "A resposta de autenticação contém um prazo inválido.",
        ));
    }
    let expires = current_time_millis()?
        .checked_add(expires_delta as i64)
        .ok_or_else(|| {
            ProviderError::new(
                "malformed_token",
                "A resposta de autenticação contém um prazo inválido.",
            )
        })?;
    let profile = token_profile(
        &access,
        value.get("id_token").and_then(serde_json::Value::as_str),
    );
    let account_id = profile
        .account_id
        .or_else(|| stored.map(|credential| credential.account_id.clone()))
        .ok_or_else(|| {
            ProviderError::new(
                "malformed_token",
                "A conta do provedor não foi identificada.",
            )
        })?;

    Ok(OAuthToken {
        access,
        refresh,
        expires,
        account_id,
        email: profile
            .email
            .or_else(|| stored.and_then(|credential| credential.email.clone())),
        plan_type: profile
            .plan_type
            .or_else(|| stored.and_then(|credential| credential.plan_type.clone())),
    })
}

fn exchange_authorization_code(
    endpoints: &OAuthEndpoints,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<OAuthToken, ProviderError> {
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    form.append_pair("grant_type", "authorization_code")
        .append_pair("client_id", OPENAI_CODEX_CLIENT_ID)
        .append_pair("code", code)
        .append_pair("code_verifier", verifier)
        .append_pair("redirect_uri", redirect_uri);
    request_oauth_token(endpoints, form.finish(), None)
}

fn refresh_credential(
    endpoints: &OAuthEndpoints,
    credential: &CodexCredential,
) -> Result<CodexCredential, ProviderError> {
    if credential.project_id.is_some() {
        return antigravity::refresh(credential);
    }
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    form.append_pair("grant_type", "refresh_token")
        .append_pair("client_id", OPENAI_CODEX_CLIENT_ID)
        .append_pair("refresh_token", &credential.refresh);
    let token = request_oauth_token(endpoints, form.finish(), Some(credential))?;
    Ok(CodexCredential::new(
        token.access,
        token.refresh,
        token.expires,
        token.account_id,
        token.email,
        token.plan_type,
    ))
}
fn build_codex_client() -> Result<reqwest::blocking::Client, reqwest::Error> {
    reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15))
        .build()
}

fn codex_json(
    client: &reqwest::blocking::Client,
    base_url: &str,
    path: &str,
    credential: &CodexCredential,
    include_model_headers: bool,
) -> Option<serde_json::Value> {
    let mut url = url::Url::parse(&format!("{}/{path}", base_url.trim_end_matches('/'))).ok()?;
    if include_model_headers {
        url.query_pairs_mut()
            .append_pair("client_version", OPENAI_CODEX_CLIENT_VERSION);
    }

    let mut request = client
        .get(url)
        .header("Authorization", format!("Bearer {}", credential.access))
        .header("accept", "application/json")
        .header("chatgpt-account-id", &credential.account_id);
    if include_model_headers {
        request = request
            .header("OpenAI-Beta", "responses=experimental")
            .header("originator", "codex_cli_rs")
            .header("version", OPENAI_CODEX_CLIENT_VERSION);
    }
    let response = request.send().ok()?;
    if !response.status().is_success() {
        return None;
    }

    use std::io::Read;
    const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
    let mut bytes = Vec::new();
    response
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

fn non_empty_model_string(value: Option<&serde_json::Value>) -> Option<&str> {
    let value = value?.as_str()?.trim();
    if value.is_empty() || value.len() > 256 {
        return None;
    }
    Some(value)
}

fn reasoning_level(value: Option<&serde_json::Value>) -> Option<&str> {
    non_empty_model_string(value).filter(|level| {
        level.len() <= 32
            && level.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
            })
    })
}

fn model_reasoning(
    entry: &serde_json::Map<String, serde_json::Value>,
) -> (Vec<String>, Option<String>) {
    let default = reasoning_level(entry.get("default_reasoning_level"));
    let mut levels = Vec::new();
    match entry.get("supported_reasoning_levels") {
        Some(serde_json::Value::Array(values)) => {
            for value in values {
                let Some(level) = reasoning_level(value.get("effort").or(Some(value))) else {
                    continue;
                };
                if !levels.contains(&level) {
                    levels.push(level);
                }
            }
        }
        // A reported default is the only known option when the list is absent.
        None => levels.extend(default),
        Some(_) => {}
    }
    let default = default.filter(|level| levels.contains(level));
    (
        levels.into_iter().map(str::to_owned).collect(),
        default.map(str::to_owned),
    )
}

fn normalize_codex_models(payload: &serde_json::Value) -> Option<Vec<ProviderModel>> {
    let payload = payload.as_object()?;
    let entries = if let Some(entries) = payload.get("models").or_else(|| payload.get("data")) {
        entries.as_array()?.as_slice()
    } else {
        &[]
    };
    let mut seen = std::collections::HashSet::with_capacity(entries.len());
    let mut models = Vec::with_capacity(entries.len());

    for entry in entries {
        let Some(entry) = entry.as_object() else {
            continue;
        };
        let Some(id) = non_empty_model_string(entry.get("slug"))
            .or_else(|| non_empty_model_string(entry.get("id")))
        else {
            continue;
        };
        let visibility = non_empty_model_string(entry.get("visibility"));
        if visibility.is_some_and(|value| {
            value.eq_ignore_ascii_case("hide") || value.eq_ignore_ascii_case("hidden")
        }) || !seen.insert(id.to_owned())
        {
            continue;
        }
        let name = non_empty_model_string(entry.get("display_name")).unwrap_or(id);
        let priority = entry
            .get("priority")
            .and_then(serde_json::Value::as_f64)
            .filter(|value| value.is_finite())
            .unwrap_or(f64::MAX);
        let (reasoning_levels, default_reasoning_level) = model_reasoning(entry);
        models.push((
            priority,
            ProviderModel {
                id: id.to_owned(),
                name: name.to_owned(),
                reasoning_levels,
                default_reasoning_level,
                context_window: entry
                    .get("context_window")
                    .and_then(serde_json::Value::as_u64)
                    .filter(|value| *value > 0 && *value <= 9_007_199_254_740_991),
            },
        ));
    }

    models.sort_by(|left, right| {
        left.0
            .total_cmp(&right.0)
            .then_with(|| left.1.id.cmp(&right.1.id))
    });
    Some(models.into_iter().map(|(_, model)| model).collect())
}

fn fetch_codex_models(
    client: &reqwest::blocking::Client,
    base_url: &str,
    credential: &CodexCredential,
) -> Option<Vec<ProviderModel>> {
    for path in ["codex/models", "models"] {
        let Some(payload) = codex_json(client, base_url, path, credential, true) else {
            continue;
        };
        if let Some(models) = normalize_codex_models(&payload) {
            return Some(models);
        }
    }
    None
}

fn fetch_plan_type(
    client: &reqwest::blocking::Client,
    base_url: &str,
    credential: &CodexCredential,
) -> Option<String> {
    let payload = codex_json(client, base_url, "wham/usage", credential, false)?;
    normalized_claim(payload.get("plan_type"), 64)
}

fn account_details(
    record: ProviderAccountRecord,
    secret_store: &dyn SecretStore,
    endpoints: &OAuthEndpoints,
    client: Option<&reqwest::blocking::Client>,
) -> ProviderAccount {
    let Ok(mut credential) = secret_store.load(&record.alias) else {
        return ProviderAccount::from_record(record, None, Vec::new(), false);
    };
    if !record.enabled {
        return ProviderAccount::from_record(record, Some(&credential), Vec::new(), false);
    }
    let original = serde_json::to_vec(&credential).ok();
    let profile = token_profile(&credential.access, None);
    credential.email = credential.email.or(profile.email);
    credential.plan_type = credential.plan_type.or(profile.plan_type);

    if current_time_millis().is_ok_and(|now| credential.expires <= now + 60_000) {
        if let Ok(refreshed) = refresh_credential(endpoints, &credential) {
            credential = refreshed;
        }
    }

    let models = client.and_then(|client| {
        if credential.project_id.is_none() && credential.plan_type.is_none() {
            credential.plan_type = fetch_plan_type(client, OPENAI_CODEX_BASE_URL, &credential);
        }
        fetch_provider_models(client, &mut credential)
    });
    if serde_json::to_vec(&credential).ok() != original {
        let _ = secret_store.store(&record.alias, &credential);
    }
    let models_available = models.is_some();
    ProviderAccount::from_record(
        record,
        Some(&credential),
        models.unwrap_or_default(),
        models_available,
    )
}

fn fetch_provider_models(
    client: &reqwest::blocking::Client,
    credential: &mut CodexCredential,
) -> Option<Vec<ProviderModel>> {
    if credential.project_id.is_some() {
        antigravity::fetch_models(client, credential)
    } else {
        fetch_codex_models(client, OPENAI_CODEX_BASE_URL, credential)
    }
}

fn commit_provider_account_with_state(
    app_state: &persistence::AppState,
    home_dir: &std::path::Path,
    secret_store: &dyn SecretStore,
    alias: &str,
    credential: &CodexCredential,
) -> Result<ProviderAccount, ProviderError> {
    app_state
        .with_connection(home_dir, |connection| {
            commit_provider_account(connection, secret_store, alias, credential)
        })
        .map_err(ProviderError::from_account_error)
}

fn disconnect_provider_account_with_state(
    app_state: &persistence::AppState,
    home_dir: &std::path::Path,
    secret_store: &dyn SecretStore,
    alias: &str,
) -> Result<(), ProviderError> {
    app_state
        .with_connection(home_dir, |connection| {
            disconnect_provider_account(connection, secret_store, alias)
        })
        .map_err(ProviderError::from_account_error)
}

fn home_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, ProviderError> {
    use tauri::Manager;
    app.path().home_dir().map_err(|_| {
        ProviderError::new(
            "home_directory_error",
            "Não foi possível acessar o diretório da aplicação.",
        )
    })
}

#[tauri::command]
pub async fn list_provider_accounts(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, persistence::AppState>,
    oauth_state: tauri::State<'_, OpenAiCodexState>,
) -> Result<Vec<ProviderAccount>, ProviderError> {
    let home_dir = home_dir(&app)?;
    let persistence_state = persistence_state.inner().clone();
    let manager = oauth_state.manager.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let client = build_codex_client().ok();
        manager.list_accounts(&persistence_state, &home_dir, client.as_ref())
    })
    .await
    .map_err(|_| ProviderError::internal())?
}

#[tauri::command]
pub async fn begin_openai_codex_connection(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, persistence::AppState>,
    oauth_state: tauri::State<'_, OpenAiCodexState>,
    alias: String,
) -> Result<OpenAiCodexConnectionStart, ProviderError> {
    let home_dir = home_dir(&app)?;
    let persistence_state = persistence_state.inner().clone();
    let manager = oauth_state.manager.clone();
    tauri::async_runtime::spawn_blocking(move || {
        manager.begin(&persistence_state, &home_dir, &alias)
    })
    .await
    .map_err(|_| ProviderError::internal())?
}

#[tauri::command]
pub async fn reauthorize_provider_account(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, persistence::AppState>,
    oauth_state: tauri::State<'_, OpenAiCodexState>,
    alias: String,
) -> Result<OpenAiCodexConnectionStart, ProviderError> {
    let home = home_dir(&app)?;
    let state = persistence_state.inner().clone();
    let manager = oauth_state.manager.clone();
    tauri::async_runtime::spawn_blocking(move || {
        manager.begin_connection(&state, &home, &alias, true)
    })
    .await
    .map_err(|_| ProviderError::internal())?
}

#[tauri::command]
pub async fn set_provider_enabled(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, persistence::AppState>,
    oauth_state: tauri::State<'_, OpenAiCodexState>,
    alias: String,
    enabled: bool,
) -> Result<(), ProviderError> {
    let home = home_dir(&app)?;
    let state = persistence_state.inner().clone();
    let manager = oauth_state.manager.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = manager
            .credentials_guard
            .lock()
            .map_err(|_| ProviderError::internal())?;
        state.with_connection(&home, |connection| {
            let changed = connection
                .execute(
                    "UPDATE provider_accounts SET enabled = ?2 WHERE alias = ?1",
                    rusqlite::params![alias, enabled],
                )
                .map_err(|_| ProviderError::database())?;
            if changed != 1 {
                return Err(ProviderError::new(
                    "account_missing",
                    "A conta não está mais cadastrada.",
                ));
            }
            Ok(())
        })
    })
    .await
    .map_err(|_| ProviderError::internal())?
}

#[tauri::command]
pub async fn wait_openai_codex_connection(
    oauth_state: tauri::State<'_, OpenAiCodexState>,
    flow_id: String,
) -> Result<ProviderAccount, ProviderError> {
    let manager = oauth_state.manager.clone();
    tauri::async_runtime::spawn_blocking(move || manager.wait(&flow_id))
        .await
        .map_err(|_| ProviderError::internal())?
}

#[tauri::command]
pub async fn cancel_openai_codex_connection(
    oauth_state: tauri::State<'_, OpenAiCodexState>,
    flow_id: String,
) -> Result<(), ProviderError> {
    let manager = oauth_state.manager.clone();
    tauri::async_runtime::spawn_blocking(move || manager.cancel(&flow_id))
        .await
        .map_err(|_| ProviderError::internal())?
}

#[tauri::command]
pub async fn disconnect_provider_account_command(
    app: tauri::AppHandle,
    persistence_state: tauri::State<'_, persistence::AppState>,
    oauth_state: tauri::State<'_, OpenAiCodexState>,
    alias: String,
) -> Result<(), ProviderError> {
    let home_dir = home_dir(&app)?;
    let persistence_state = persistence_state.inner().clone();
    let manager = oauth_state.manager.clone();
    tauri::async_runtime::spawn_blocking(move || {
        manager.disconnect_account(&persistence_state, &home_dir, &alias)
    })
    .await
    .map_err(|_| ProviderError::internal())?
}

#[cfg(test)]
mod oauth_tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        path::PathBuf,
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        },
        thread::JoinHandle,
        time::Duration,
    };

    #[test]
    fn disabled_account_is_preserved_and_rejected_before_keychain_or_network() {
        let home = test_home();
        let state = persistence::AppState::default();
        state
            .with_connection(&home, |connection| {
                let record = persistence::insert_provider_account(
                    connection,
                    "openai-codex-disabled",
                    "fixture-account",
                )?;
                assert!(record.enabled);
                connection.execute(
                    "UPDATE provider_accounts SET enabled = 0 WHERE alias = ?1",
                    [&record.alias],
                )?;
                Ok::<_, PersistenceError>(())
            })
            .unwrap();
        let oauth = OpenAiCodexState::default();
        let err = oauth
            .credential_and_models(&state, &home, "openai-codex-disabled")
            .err()
            .unwrap();
        assert_eq!(err.code, "account_disabled");
        let records = state.list_provider_accounts(&home).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].account_id, "fixture-account");
        assert!(!records[0].enabled);
        assert!(persistence::require_enabled_account(&state, &home, &records[0].alias).is_err());
        drop(state);
        std::fs::remove_dir_all(home).unwrap();
    }

    static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

    fn test_home() -> PathBuf {
        let id = NEXT_HOME.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "jarvis-openai-codex-oauth-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("test home");
        path
    }

    fn free_port() -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("free port");
        listener.local_addr().expect("free port address").port()
    }

    #[test]
    fn disconnect_waits_for_account_enrichment_and_never_recreates_removed_credentials() {
        use base64::Engine;
        use std::sync::{mpsc, Mutex};

        struct PausedLoadStore {
            inner: InMemorySecretStore,
            loaded: mpsc::Sender<()>,
            resume: Mutex<mpsc::Receiver<()>>,
        }
        impl SecretStore for PausedLoadStore {
            fn load(&self, alias: &str) -> Result<CodexCredential, SecretStoreError> {
                let credential = self.inner.load(alias)?;
                self.loaded.send(()).expect("load notification");
                self.resume
                    .lock()
                    .expect("resume lock")
                    .recv_timeout(Duration::from_secs(5))
                    .expect("resume load");
                Ok(credential)
            }
            fn store(
                &self,
                alias: &str,
                credential: &CodexCredential,
            ) -> Result<(), SecretStoreError> {
                self.inner.store(alias, credential)
            }
            fn remove(&self, alias: &str) -> Result<(), SecretStoreError> {
                self.inner.remove(alias)
            }
        }

        let home = test_home();
        let state = persistence::AppState::default();
        let (loaded_tx, loaded_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let secrets = Arc::new(PausedLoadStore {
            inner: InMemorySecretStore::default(),
            loaded: loaded_tx,
            resume: Mutex::new(resume_rx),
        });
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&serde_json::json!({
                OPENAI_CODEX_PROFILE_CLAIM: { "email": "person@example.com" }
            }))
            .expect("synthetic profile"),
        );
        let credential = CodexCredential::new(
            format!("header.{payload}.signature"),
            "refresh",
            i64::MAX,
            "account-one",
            None,
            None,
        );
        let alias = "openai-codex-race";
        commit_provider_account_with_state(&state, &home, secrets.as_ref(), alias, &credential)
            .expect("connect");
        let manager = Arc::new(OAuthManager::production(secrets.clone()));

        let list_manager = manager.clone();
        let list_state = state.clone();
        let list_home = home.clone();
        let listing =
            std::thread::spawn(move || list_manager.list_accounts(&list_state, &list_home, None));
        loaded_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("listing loaded credentials");

        let remove_manager = manager.clone();
        let remove_state = state.clone();
        let remove_home = home.clone();
        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let removal = std::thread::spawn(move || {
            started_tx.send(()).expect("disconnect started");
            let result = remove_manager.disconnect_account(&remove_state, &remove_home, alias);
            done_tx.send(()).expect("disconnect done");
            result
        });
        started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("disconnect attempt");
        let disconnected_early = done_rx.recv_timeout(Duration::from_millis(100)).is_ok();
        resume_tx.send(()).expect("resume listing");
        let accounts = listing
            .join()
            .expect("listing worker")
            .expect("listing result");
        removal
            .join()
            .expect("disconnect worker")
            .expect("disconnect result");

        assert!(
            !disconnected_early,
            "disconnect must wait until credential writes complete"
        );
        assert_eq!(accounts[0].email.as_deref(), Some("person@example.com"));
        assert!(matches!(
            secrets.inner.load(alias),
            Err(SecretStoreError::Missing)
        ));
        assert!(state
            .list_provider_accounts(&home)
            .expect("remaining accounts")
            .is_empty());
        state.close();
        std::fs::remove_dir_all(home).expect("remove test home");
    }

    fn new_manager(
        token_url: String,
        timeout: Duration,
        secret_store: Arc<InMemorySecretStore>,
        callback_ports: Vec<u16>,
    ) -> Arc<OAuthManager> {
        Arc::new(OAuthManager::new(
            OAuthEndpoints::test(OPENAI_CODEX_AUTHORIZE_URL, token_url),
            callback_ports,
            timeout,
            secret_store,
        ))
    }

    fn redirect_port(start: &OpenAiCodexConnectionStart) -> u16 {
        let authorization = url::Url::parse(&start.authorization_url).expect("authorization URL");
        let redirect = authorization
            .query_pairs()
            .find(|(key, _)| key == "redirect_uri")
            .map(|(_, value)| value.into_owned())
            .expect("redirect URI");
        url::Url::parse(&redirect)
            .expect("redirect URL")
            .port()
            .expect("redirect port")
    }

    fn callback_state(start: &OpenAiCodexConnectionStart) -> String {
        url::Url::parse(&start.authorization_url)
            .expect("authorization URL")
            .query_pairs()
            .find(|(key, _)| key == "state")
            .map(|(_, value)| value.into_owned())
            .expect("callback state")
    }

    fn request(start: &OpenAiCodexConnectionStart, target: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", redirect_port(start))).expect("callback");
        write!(
            stream,
            "GET {target} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
        )
        .expect("callback request");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("callback response");
        response
    }

    fn jwt(account_id: &str) -> String {
        use base64::Engine;
        let payload = serde_json::json!({
            OPENAI_CODEX_AUTH_CLAIM: { "chatgpt_account_id": account_id }
        });
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).expect("JWT payload"));
        format!("header.{payload}.signature")
    }

    fn fake_token_server(
        response_body: String,
        status: &'static str,
    ) -> (String, JoinHandle<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("fake token endpoint");
        let port = listener
            .local_addr()
            .expect("token endpoint address")
            .port();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("token request");
            let mut request = Vec::new();
            let mut chunk = [0_u8; 1024];
            let header_end;
            loop {
                let read = stream.read(&mut chunk).expect("token request read");
                assert!(read > 0, "token request ended before headers");
                request.extend_from_slice(&chunk[..read]);
                if let Some(position) = request.windows(4).position(|window| window == b"\r\n\r\n")
                {
                    header_end = position + 4;
                    break;
                }
            }
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Content-Length:")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            while request.len() < header_end + content_length {
                let read = stream.read(&mut chunk).expect("token body read");
                assert!(read > 0, "token request body ended early");
                request.extend_from_slice(&chunk[..read]);
            }

            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream
                .write_all(response.as_bytes())
                .expect("token response");
            String::from_utf8(request).expect("token request UTF-8")
        });
        (format!("http://127.0.0.1:{port}/oauth/token"), handle)
    }

    fn redirecting_token_server(redirect_url: String) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("fake token endpoint");
        let port = listener
            .local_addr()
            .expect("token endpoint address")
            .port();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("token request");
            let response = format!(
                "HTTP/1.1 302 Found\r\nLocation: {redirect_url}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream
                .write_all(response.as_bytes())
                .expect("redirect response");
        });
        (format!("http://127.0.0.1:{port}/oauth/token"), handle)
    }

    fn begin(
        manager: &Arc<OAuthManager>,
        app_state: &persistence::AppState,
        home: &std::path::Path,
        alias: &str,
    ) -> OpenAiCodexConnectionStart {
        manager
            .begin(app_state, home, alias)
            .expect("begin OAuth flow")
    }

    #[test]
    fn authorization_url_has_exact_protocol_shape_without_verifier() {
        let home = test_home();
        let app_state = persistence::AppState::default();
        let secrets = Arc::new(InMemorySecretStore::default());
        let manager = new_manager(
            "http://127.0.0.1:9/oauth/token".to_owned(),
            Duration::from_secs(1),
            secrets,
            vec![free_port()],
        );
        let start = begin(&manager, &app_state, &home, "openai-codex-url");
        let authorization = url::Url::parse(&start.authorization_url).expect("authorization URL");
        let pairs = authorization
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            authorization.origin().ascii_serialization(),
            "https://auth.openai.com"
        );
        assert_eq!(
            pairs.get("response_type").map(|value| value.as_ref()),
            Some("code")
        );
        assert_eq!(
            pairs.get("client_id").map(|value| value.as_ref()),
            Some(OPENAI_CODEX_CLIENT_ID)
        );
        assert_eq!(
            pairs.get("scope").map(|value| value.as_ref()),
            Some(OPENAI_CODEX_SCOPE)
        );
        assert_eq!(
            pairs
                .get("code_challenge_method")
                .map(|value| value.as_ref()),
            Some("S256")
        );
        assert_eq!(
            pairs
                .get("id_token_add_organizations")
                .map(|value| value.as_ref()),
            Some("true")
        );
        assert_eq!(
            pairs
                .get("codex_cli_simplified_flow")
                .map(|value| value.as_ref()),
            Some("true")
        );
        assert_eq!(
            pairs.get("originator").map(|value| value.as_ref()),
            Some("jarvis")
        );
        assert!(!start.authorization_url.contains("verifier"));
        let state = callback_state(&start);
        assert!(!state.is_empty());
        manager.cancel(&start.flow_id).expect("cancel flow");
        assert_eq!(
            manager
                .flow(&start.flow_id)
                .err()
                .expect("cancelled flow must be reaped")
                .code,
            "flow_not_found"
        );
        manager.cancel(&start.flow_id).expect("repeat cancel");
        app_state.close();
        std::fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn callback_allows_only_route_and_matching_state() {
        let home = test_home();
        let app_state = persistence::AppState::default();
        let secrets = Arc::new(InMemorySecretStore::default());
        let manager = new_manager(
            "http://127.0.0.1:9/oauth/token".to_owned(),
            Duration::from_secs(1),
            secrets,
            vec![free_port()],
        );
        let start = begin(&manager, &app_state, &home, "openai-codex-callback");
        assert!(request(&start, "/unknown").starts_with("HTTP/1.1 404 Not Found"));
        let rejected = request(&start, "/auth/callback?state=wrong&code=authorization-code");
        assert!(rejected.starts_with("HTTP/1.1 400 Bad Request"));
        assert!(rejected.contains("Não foi possível autenticar"));
        assert!(rejected.contains("id=\"close-tab\" type=\"button\""));
        assert!(rejected.contains("id=\"close-feedback\" role=\"status\""));
        assert!(rejected.contains("Seu navegador bloqueou o fechamento pelo botão."));
        assert!(!rejected.contains("authorization-code"));
        assert_eq!(
            manager.wait(&start.flow_id).expect_err("state result").code,
            "callback_state"
        );
        let accounts = app_state
            .list_provider_accounts(&home)
            .expect("account list");
        assert!(accounts.is_empty());
        app_state.close();
        std::fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn token_exchange_does_not_follow_redirects() {
        let target_listener = TcpListener::bind(("127.0.0.1", 0)).expect("redirect target");
        let target_port = target_listener
            .local_addr()
            .expect("redirect target address")
            .port();
        let target_handle = std::thread::spawn(move || {
            target_listener
                .set_nonblocking(true)
                .expect("redirect target nonblocking");
            let deadline = std::time::Instant::now() + Duration::from_millis(500);
            loop {
                match target_listener.accept() {
                    Ok(_) => return true,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => return false,
                }
            }
        });

        let (token_url, token_server) =
            redirecting_token_server(format!("http://127.0.0.1:{target_port}/oauth/token"));
        let endpoints = OAuthEndpoints::test(OPENAI_CODEX_AUTHORIZE_URL, token_url);
        let error = exchange_authorization_code(
            &endpoints,
            "AUTHORIZATION_CODE",
            "PKCE_VERIFIER",
            "http://localhost:1455/auth/callback",
        )
        .err()
        .expect("redirect response must fail token exchange");

        assert_eq!(error.code, "token_exchange");
        token_server.join().expect("token server");
        assert!(
            !target_handle.join().expect("redirect target"),
            "token request followed an unsafe redirect"
        );
    }

    #[test]
    fn reauthorization_keeps_old_credentials_until_success_and_survives_cancellation() {
        let home = test_home();
        let state = persistence::AppState::default();
        let secrets = Arc::new(InMemorySecretStore::default());
        let alias = "openai-codex-reauthorize";
        let old = CodexCredential::new(
            "old-access",
            "old-refresh",
            i64::MAX,
            "existing-account",
            None,
            Some("plus".into()),
        );
        commit_provider_account_with_state(&state, &home, secrets.as_ref(), alias, &old).unwrap();
        let original = state.list_provider_accounts(&home).unwrap();
        let access = jwt("existing-account");
        let (token_url, server) = fake_token_server(
            serde_json::json!({
                "access_token": access, "refresh_token": "new-refresh", "expires_in": 3600
            })
            .to_string(),
            "200 OK",
        );
        let manager = new_manager(
            token_url,
            Duration::from_secs(5),
            secrets.clone(),
            vec![free_port()],
        );
        assert_eq!(
            manager.begin(&state, &home, alias).unwrap_err().code,
            "duplicate_account"
        );
        let cancelled = manager
            .begin_connection(&state, &home, alias, true)
            .unwrap();
        let pending = manager.flow(&cancelled.flow_id).unwrap();
        manager.cancel(&cancelled.flow_id).unwrap();
        assert_eq!(pending.wait().unwrap_err().code, "cancelled");
        assert_eq!(secrets.load(alias).unwrap().access, "old-access");
        assert_eq!(state.list_provider_accounts(&home).unwrap(), original);

        // Use a fresh ephemeral callback port; cancellation may still be releasing the first listener.
        let manager = new_manager(
            manager.endpoints.token_url.clone(),
            Duration::from_secs(5),
            secrets.clone(),
            vec![free_port()],
        );
        let start = manager
            .begin_connection(&state, &home, alias, true)
            .unwrap();
        assert_eq!(secrets.load(alias).unwrap().refresh, "old-refresh");
        request(
            &start,
            &format!(
                "/auth/callback?state={}&code=AUTHORIZATION_CODE",
                callback_state(&start)
            ),
        );
        assert_eq!(manager.wait(&start.flow_id).unwrap().alias, alias);
        server.join().unwrap();
        assert_eq!(state.list_provider_accounts(&home).unwrap(), original);
        let renewed = secrets.load(alias).unwrap();
        assert_eq!(renewed.access, access);
        assert_eq!(renewed.refresh, "new-refresh");
        state.close();
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn reauthorization_token_failure_preserves_existing_account() {
        let home = test_home();
        let state = persistence::AppState::default();
        let secrets = Arc::new(InMemorySecretStore::default());
        let alias = "openai-codex-reauthorize";
        let old = CodexCredential::new(
            "old-access",
            "old-refresh",
            i64::MAX,
            "existing-account",
            None,
            None,
        );
        commit_provider_account_with_state(&state, &home, secrets.as_ref(), alias, &old).unwrap();
        let original = state.list_provider_accounts(&home).unwrap();
        let (token_url, server) = fake_token_server("{}".into(), "400 Bad Request");
        let manager = new_manager(
            token_url,
            Duration::from_secs(5),
            secrets.clone(),
            vec![free_port()],
        );
        let start = manager
            .begin_connection(&state, &home, alias, true)
            .unwrap();
        request(
            &start,
            &format!(
                "/auth/callback?state={}&code=AUTHORIZATION_CODE",
                callback_state(&start)
            ),
        );
        assert_eq!(
            manager.wait(&start.flow_id).unwrap_err().code,
            "token_exchange"
        );
        server.join().unwrap();
        assert_eq!(state.list_provider_accounts(&home).unwrap(), original);
        assert_eq!(secrets.load(alias).unwrap().access, "old-access");
        state.close();
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn fake_issuer_success_commits_metadata_and_secret_without_redaction_leaks() {
        let access = jwt("account-success");
        let refresh = "REFRESH_SECRET";
        let (token_url, token_server) = fake_token_server(
            serde_json::json!({
                "access_token": access,
                "refresh_token": refresh,
                "expires_in": 3600
            })
            .to_string(),
            "200 OK",
        );
        let home = test_home();
        let app_state = persistence::AppState::default();
        let secrets = Arc::new(InMemorySecretStore::default());
        let manager = new_manager(
            token_url,
            Duration::from_secs(2),
            secrets.clone(),
            vec![free_port()],
        );
        let start = begin(&manager, &app_state, &home, "openai-codex-success");
        let state = callback_state(&start);
        let callback_response = request(
            &start,
            &format!("/auth/callback?state={state}&code=AUTHORIZATION_CODE"),
        );
        assert!(callback_response.starts_with("HTTP/1.1 200 OK"));
        assert!(callback_response.contains("Jarvis — Autenticação"));
        assert!(callback_response.contains("Autenticação recebida"));
        assert!(callback_response.contains("Você pode fechar esta aba"));
        assert!(callback_response.contains("id=\"close-tab\" type=\"button\""));
        assert!(callback_response.contains("Fechar aba"));
        assert!(callback_response.contains("Seu navegador bloqueou o fechamento pelo botão."));
        // The page must carry the current brand mark and drop the stale chevron.
        assert!(callback_response.contains("data:image/png;base64,"));
        assert!(!callback_response.contains("jarvisChevron"));
        let account = manager.wait(&start.flow_id).expect("OAuth success");
        let token_request = token_server.join().expect("token server");
        assert!(token_request.contains("grant_type=authorization_code"));
        assert!(token_request.contains(&format!("client_id={OPENAI_CODEX_CLIENT_ID}")));
        assert!(token_request.contains("code=AUTHORIZATION_CODE"));
        assert!(token_request.contains("code_verifier="));
        assert!(token_request.contains("redirect_uri=http%3A%2F%2Flocalhost%3A"));

        let records = app_state
            .list_provider_accounts(&home)
            .expect("account list");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].account_id, "account-success");
        let credential = secrets.load("openai-codex-success").expect("credential");
        assert_eq!(credential.access, access);
        assert_eq!(credential.refresh, refresh);
        assert_eq!(credential.account_id, "account-success");
        let serialized = serde_json::to_string(&account).expect("account JSON");
        assert!(!serialized.contains("REFRESH_SECRET"));
        assert!(!serialized.contains("AUTHORIZATION_CODE"));
        assert!(!serialized.contains(&state));
        assert!(serde_json::to_string(&ProviderError::new(
            "malformed_token",
            "A resposta de autenticação é inválida.",
        ))
        .expect("error JSON")
        .find("REFRESH_SECRET")
        .is_none());
        app_state.close();
        std::fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn cancel_during_commit_returns_durable_success() {
        let access = jwt("account-cancel-race");
        let (token_url, token_server) = fake_token_server(
            serde_json::json!({
                "access_token": access,
                "refresh_token": "REFRESH_SECRET",
                "expires_in": 3600
            })
            .to_string(),
            "200 OK",
        );
        let home = test_home();
        let app_state = persistence::AppState::default();
        let secrets = Arc::new(InMemorySecretStore::default());
        let entries = secrets.entries.lock().expect("secret store gate");
        let manager = new_manager(
            token_url,
            Duration::from_secs(2),
            secrets.clone(),
            vec![free_port()],
        );
        let start = begin(&manager, &app_state, &home, "openai-codex-cancel-race");
        request(
            &start,
            &format!(
                "/auth/callback?state={}&code=AUTHORIZATION_CODE",
                callback_state(&start)
            ),
        );
        token_server.join().expect("token server");
        let flow = manager.flow(&start.flow_id).expect("active flow");
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            match flow.commit_guard.try_lock() {
                Ok(guard) => drop(guard),
                Err(std::sync::TryLockError::WouldBlock) => break,
                Err(std::sync::TryLockError::Poisoned(_)) => panic!("commit guard poisoned"),
            }
            assert!(std::time::Instant::now() < deadline, "commit did not start");
            std::thread::yield_now();
        }

        let manager_for_wait = manager.clone();
        let flow_id = start.flow_id.clone();
        let wait = std::thread::spawn(move || manager_for_wait.wait(&flow_id));
        while !flow.wait_started.load(std::sync::atomic::Ordering::Acquire) {
            assert!(std::time::Instant::now() < deadline, "wait did not start");
            std::thread::yield_now();
        }
        let manager_for_cancel = manager.clone();
        let flow_id = start.flow_id.clone();
        let cancel = std::thread::spawn(move || manager_for_cancel.cancel(&flow_id));
        let cancel_deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !flow.cancelled.load(std::sync::atomic::Ordering::Acquire) {
            assert!(
                std::time::Instant::now() < cancel_deadline,
                "cancel did not start"
            );
            std::thread::yield_now();
        }
        drop(entries);

        let account = wait.join().expect("wait thread").expect("durable success");
        cancel.join().expect("cancel thread").expect("cancel flow");
        assert_eq!(account.alias, "openai-codex-cancel-race");
        assert_eq!(
            app_state
                .list_provider_accounts(&home)
                .expect("account list")
                .len(),
            1
        );
        assert!(secrets.load("openai-codex-cancel-race").is_ok());
        app_state.close();
        std::fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn denial_timeout_cancellation_and_one_active_flow_never_create_accounts() {
        let home = test_home();
        let app_state = persistence::AppState::default();
        let secrets = Arc::new(InMemorySecretStore::default());
        let manager = new_manager(
            "http://127.0.0.1:9/oauth/token".to_owned(),
            Duration::from_secs(5),
            secrets.clone(),
            vec![free_port(), free_port(), free_port()],
        );
        let first = begin(&manager, &app_state, &home, "openai-codex-first");
        let second = manager
            .begin(&app_state, &home, "openai-codex-second")
            .expect_err("one active flow");
        assert_eq!(second.code, "active_flow");
        let flow = manager.flow(&first.flow_id).expect("active flow");
        let manager_for_wait = manager.clone();
        let flow_id = first.flow_id.clone();
        let wait = std::thread::spawn(move || manager_for_wait.wait(&flow_id));
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !flow.wait_started.load(std::sync::atomic::Ordering::Acquire) {
            assert!(std::time::Instant::now() < deadline, "wait did not start");
            std::thread::yield_now();
        }
        manager.cancel(&first.flow_id).expect("cancel flow");
        assert_eq!(
            wait.join()
                .expect("wait thread")
                .expect_err("cancel result")
                .code,
            "cancelled"
        );
        assert!(manager.flow(&first.flow_id).is_err());

        // Only the expiry scenario should race a short deadline. Cancellation and
        // denial must test their outcomes even on a loaded CI runner.
        let timeout_manager = new_manager(
            "http://127.0.0.1:9/oauth/token".to_owned(),
            Duration::from_millis(60),
            secrets,
            vec![free_port()],
        );
        let timeout_flow = begin(&timeout_manager, &app_state, &home, "openai-codex-timeout");
        let timeout_error = timeout_manager
            .wait(&timeout_flow.flow_id)
            .expect_err("timeout result");
        assert_eq!(timeout_error.code, "timeout");
        timeout_manager
            .cancel(&timeout_flow.flow_id)
            .expect("cancel settled flow");
        timeout_manager
            .cancel(&timeout_flow.flow_id)
            .expect("repeat cancel settled flow");
        let denial_flow = begin(&manager, &app_state, &home, "openai-codex-denial");
        let response = request(
            &denial_flow,
            &format!(
                "/auth/callback?state={}&error=access_denied",
                callback_state(&denial_flow)
            ),
        );
        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
        assert_eq!(
            manager
                .wait(&denial_flow.flow_id)
                .expect_err("denial result")
                .code,
            "denied"
        );
        assert!(app_state
            .list_provider_accounts(&home)
            .expect("account list")
            .is_empty());
        app_state.close();
        std::fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn malformed_tokens_and_secret_failures_leave_no_metadata() {
        let (token_url, token_server) = fake_token_server(
            r#"{"access_token":"","refresh_token":"REFRESH_SECRET","expires_in":3600}"#.to_owned(),
            "200 OK",
        );
        let home = test_home();
        let app_state = persistence::AppState::default();
        let secrets = Arc::new(InMemorySecretStore::default());
        let manager = new_manager(
            token_url,
            Duration::from_secs(2),
            secrets.clone(),
            vec![free_port()],
        );
        let malformed = begin(&manager, &app_state, &home, "openai-codex-malformed");
        request(
            &malformed,
            &format!(
                "/auth/callback?state={}&code=code",
                callback_state(&malformed)
            ),
        );
        let error = manager
            .wait(&malformed.flow_id)
            .expect_err("malformed token");
        token_server.join().expect("token server");
        assert_eq!(error.code, "malformed_token");
        assert!(!error.message.contains("REFRESH_SECRET"));
        assert!(app_state
            .list_provider_accounts(&home)
            .expect("account list")
            .is_empty());

        let access = jwt("account-secret-failure");
        let (token_url, token_server) = fake_token_server(
            serde_json::json!({
                "access_token": access,
                "refresh_token": "REFRESH_SECRET",
                "expires_in": 3600
            })
            .to_string(),
            "200 OK",
        );
        secrets.fail_store(true);
        let manager = new_manager(
            token_url,
            Duration::from_secs(2),
            secrets,
            vec![free_port()],
        );
        let failed = begin(&manager, &app_state, &home, "openai-codex-secret-failure");
        request(
            &failed,
            &format!("/auth/callback?state={}&code=code", callback_state(&failed)),
        );
        assert_eq!(
            manager
                .wait(&failed.flow_id)
                .expect_err("secret store failure")
                .code,
            "secret_store"
        );
        token_server.join().expect("token server");
        assert!(app_state
            .list_provider_accounts(&home)
            .expect("account list")
            .is_empty());
        app_state.close();
        std::fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn duplicate_account_port_conflict_and_disconnect_are_safe() {
        let access = jwt("account-duplicate");
        let (token_url, token_server) = fake_token_server(
            serde_json::json!({
                "access_token": access,
                "refresh_token": "REFRESH_ONE",
                "expires_in": 3600
            })
            .to_string(),
            "200 OK",
        );
        let home = test_home();
        let app_state = persistence::AppState::default();
        let secrets = Arc::new(InMemorySecretStore::default());
        let manager = new_manager(
            token_url,
            Duration::from_secs(2),
            secrets.clone(),
            vec![free_port()],
        );
        let first = begin(&manager, &app_state, &home, "openai-codex-duplicate-one");
        request(
            &first,
            &format!("/auth/callback?state={}&code=code", callback_state(&first)),
        );
        manager.wait(&first.flow_id).expect("first account");
        token_server.join().expect("token server");

        let access = jwt("account-duplicate");
        let (token_url, token_server) = fake_token_server(
            serde_json::json!({
                "access_token": access,
                "refresh_token": "REFRESH_TWO",
                "expires_in": 3600
            })
            .to_string(),
            "200 OK",
        );
        let manager = new_manager(
            token_url,
            Duration::from_secs(2),
            secrets.clone(),
            vec![free_port()],
        );
        let duplicate = begin(&manager, &app_state, &home, "openai-codex-duplicate-two");
        request(
            &duplicate,
            &format!(
                "/auth/callback?state={}&code=code",
                callback_state(&duplicate)
            ),
        );
        assert_eq!(
            manager
                .wait(&duplicate.flow_id)
                .expect_err("duplicate account")
                .code,
            "duplicate_account"
        );
        token_server.join().expect("token server");
        assert!(matches!(
            secrets.load("openai-codex-duplicate-two"),
            Err(SecretStoreError::Missing)
        ));

        let port_one = free_port();
        let port_two = free_port();
        let listener_one = TcpListener::bind(("127.0.0.1", port_one)).expect("port one");
        let listener_two = TcpListener::bind(("127.0.0.1", port_two)).expect("port two");
        let blocked_manager = new_manager(
            "http://127.0.0.1:9/oauth/token".to_owned(),
            Duration::from_secs(1),
            secrets.clone(),
            vec![port_one, port_two],
        );
        let error = blocked_manager
            .begin(&app_state, &home, "openai-codex-blocked")
            .expect_err("port conflict");
        assert_eq!(error.code, "port_unavailable");
        drop(listener_one);
        drop(listener_two);

        disconnect_provider_account_with_state(
            &app_state,
            &home,
            secrets.as_ref(),
            "openai-codex-duplicate-one",
        )
        .expect("disconnect");
        assert!(secrets.load("openai-codex-duplicate-one").is_err());
        assert!(app_state
            .list_provider_accounts(&home)
            .expect("account list")
            .is_empty());
        disconnect_provider_account_with_state(
            &app_state,
            &home,
            secrets.as_ref(),
            "openai-codex-duplicate-one",
        )
        .expect("idempotent disconnect");
        app_state.close();
        std::fs::remove_dir_all(home).expect("remove test home");
    }
}
