//! Replace OAuth credentials without replacing the account's alias or preferences.
use super::{
    validate_provider_alias, CodexCredential, ProviderAccount, ProviderAccountError, ProviderError,
    SecretStore,
};
use crate::persistence::{self, ProviderAccountRecord};
use rusqlite::{params, Connection};

pub(super) fn account_changed() -> ProviderError {
    ProviderError::new(
        "account_changed",
        "O cadastro do provedor mudou. Abra os detalhes e tente novamente.",
    )
}

pub(super) fn replace_account(
    connection: &mut Connection,
    secrets: &dyn SecretStore,
    expected: &ProviderAccountRecord,
    credential: &CodexCredential,
) -> Result<ProviderAccount, ProviderError> {
    let alias = &expected.alias;
    validate_provider_alias(alias).map_err(|_| ProviderError::invalid_alias())?;
    if credential.account_id.is_empty()
        || alias.starts_with("antigravity-") != credential.project_id.is_some()
    {
        return Err(ProviderError::from_account_error(
            ProviderAccountError::InvalidAccountId,
        ));
    }

    let transaction = connection
        .transaction()
        .map_err(|_| ProviderError::database())?;
    let records = persistence::list_provider_accounts(&transaction)?;
    let mut record = records
        .iter()
        .find(|record| record.alias == *alias)
        .cloned()
        .ok_or_else(account_changed)?;
    // A login begun before disconnection must not recreate or overwrite a newer account.
    // Read preferences again here so changes made while the browser is open survive.
    if record.account_id != expected.account_id
        || record.created_at != expected.created_at
        || record.provider_kind != expected.provider_kind
    {
        return Err(account_changed());
    }
    if records
        .iter()
        .any(|record| record.alias != *alias && record.account_id == credential.account_id)
    {
        return Err(ProviderError::from_account_error(
            ProviderAccountError::DuplicateAccount,
        ));
    }

    transaction
        .execute(
            "UPDATE provider_accounts SET account_id = ?1 WHERE alias = ?2",
            params![credential.account_id, alias],
        )
        .map_err(|_| ProviderError::database())?;

    // Missing or unreadable credentials are also recoverable through a fresh login.
    let previous = secrets.load(alias).ok();
    secrets
        .store(alias, credential)
        .map_err(|error| ProviderError::from_account_error(error.into()))?;
    if transaction.commit().is_err() {
        let rollback = match previous {
            Some(previous) => secrets.store(alias, &previous),
            None => secrets.remove(alias),
        };
        return Err(ProviderError::from_account_error(if rollback.is_ok() {
            ProviderAccountError::Database
        } else {
            ProviderAccountError::SecretCleanup
        }));
    }
    record.account_id = credential.account_id.clone();
    Ok(ProviderAccount::from_record(
        record,
        Some(credential),
        Vec::new(),
        false,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai_codex::{commit_provider_account, InMemorySecretStore};

    fn setup() -> (Connection, InMemorySecretStore, ProviderAccountRecord) {
        let mut connection = Connection::open_in_memory().unwrap();
        persistence::initialize_database(&mut connection).unwrap();
        let secrets = InMemorySecretStore::default();
        commit_provider_account(
            &connection,
            &secrets,
            "openai-codex-personal",
            &credential("old-account", "old-token"),
        )
        .unwrap();
        let record = persistence::list_provider_accounts(&connection)
            .unwrap()
            .remove(0);
        (connection, secrets, record)
    }

    fn credential(id: &str, access: &str) -> CodexCredential {
        CodexCredential::new(
            access,
            "refresh",
            i64::MAX,
            id,
            Some("user@example.com".into()),
            Some("pro".into()),
        )
    }

    #[test]
    fn replaces_authorization_and_preserves_alias_date_and_latest_preferences() {
        let (mut connection, secrets, expected) = setup();
        connection.execute("UPDATE provider_accounts SET enabled = 0, show_usage = 0, show_third_party_usage = 1", []).unwrap();
        let account = replace_account(
            &mut connection,
            &secrets,
            &expected,
            &credential("new-account", "new-token"),
        )
        .unwrap();
        assert_eq!(account.alias, expected.alias);
        assert_eq!(account.created_at, expected.created_at);
        assert!(!account.enabled);
        assert!(!account.show_usage);
        assert!(account.show_third_party_usage);
        let records = persistence::list_provider_accounts(&connection).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].account_id, "new-account");
        assert_eq!(secrets.load(&expected.alias).unwrap().access, "new-token");
    }

    #[test]
    fn repairs_missing_credentials_without_recreating_metadata() {
        let (mut connection, secrets, expected) = setup();
        secrets.remove(&expected.alias).unwrap();
        replace_account(
            &mut connection,
            &secrets,
            &expected,
            &credential("old-account", "new-token"),
        )
        .unwrap();
        assert_eq!(
            persistence::list_provider_accounts(&connection).unwrap(),
            vec![expected.clone()]
        );
        assert_eq!(secrets.load(&expected.alias).unwrap().access, "new-token");
    }

    #[test]
    fn duplicate_identity_and_missing_target_leave_credentials_untouched() {
        let (mut connection, secrets, expected) = setup();
        commit_provider_account(
            &connection,
            &secrets,
            "openai-codex-other",
            &credential("other-account", "other-token"),
        )
        .unwrap();
        let error = replace_account(
            &mut connection,
            &secrets,
            &expected,
            &credential("other-account", "new-token"),
        )
        .unwrap_err();
        assert_eq!(error.code, "duplicate_account");
        assert_eq!(secrets.load(&expected.alias).unwrap().access, "old-token");
        persistence::delete_provider_account(&connection, &expected.alias).unwrap();
        secrets.remove(&expected.alias).unwrap();
        let error = replace_account(
            &mut connection,
            &secrets,
            &expected,
            &credential("new-account", "new-token"),
        )
        .unwrap_err();
        assert_eq!(error.code, "account_changed");
        assert!(secrets.load(&expected.alias).is_err());
        assert_eq!(
            persistence::list_provider_accounts(&connection)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn secret_write_failure_rolls_back_account_identity() {
        let (mut connection, secrets, expected) = setup();
        secrets.fail_store(true);
        assert!(replace_account(
            &mut connection,
            &secrets,
            &expected,
            &credential("new-account", "new-token")
        )
        .is_err());
        assert_eq!(
            persistence::list_provider_accounts(&connection).unwrap(),
            vec![expected.clone()]
        );
        assert_eq!(secrets.load(&expected.alias).unwrap().access, "old-token");
    }

    #[test]
    fn failed_database_commit_restores_previous_credentials_and_identity() {
        let (mut connection, secrets, expected) = setup();
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE missing_parent (id INTEGER PRIMARY KEY);
             CREATE TABLE deferred_child (id INTEGER REFERENCES missing_parent(id) DEFERRABLE INITIALLY DEFERRED);
             CREATE TRIGGER fail_reauthorization AFTER UPDATE ON provider_accounts BEGIN
                 INSERT INTO deferred_child VALUES (1);
             END;"
        ).unwrap();
        let error = replace_account(
            &mut connection,
            &secrets,
            &expected,
            &credential("new-account", "new-token"),
        )
        .unwrap_err();
        assert_eq!(error.code, "database");
        assert_eq!(
            persistence::list_provider_accounts(&connection).unwrap(),
            vec![expected.clone()]
        );
        assert_eq!(secrets.load(&expected.alias).unwrap().access, "old-token");
    }
}
