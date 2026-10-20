use super::*;
use crate::openai_codex::{InMemorySecretStore, OAuthManager};
use std::sync::Arc;

fn database() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::persistence::initialize_database(&mut db).unwrap();
    db
}

#[test]
fn keys_rotate_without_changing_identity_and_never_enter_public_metadata() {
    let home = tempfile::tempdir().unwrap();
    let state = AppState::default();
    let store = Arc::new(InMemorySecretStore::default());
    let alias = "brave-search-work";
    state
        .with_connection(home.path(), |db| {
            save(
                db,
                store.as_ref(),
                alias,
                Some("brave-original-key"),
                false,
                Some("Brave Search"),
            )
        })
        .unwrap();
    let first = state.list_provider_accounts(home.path()).unwrap().remove(0);
    assert!(!first.show_usage);
    let public = serde_json::to_string(&account(first.clone())).unwrap();
    assert!(!public.contains("brave-original-key"));
    assert_eq!(account(first.clone()).models.len(), 0);
    assert!(account(first.clone()).models_available);
    state
        .with_connection(home.path(), |db| {
            save(
                db,
                store.as_ref(),
                alias,
                Some("brave-rotated-key"),
                true,
                Some("Pesquisa"),
            )
        })
        .unwrap();
    let rotated = state.list_provider_accounts(home.path()).unwrap().remove(0);
    assert_eq!(first.account_id, rotated.account_id);
    assert_eq!(first.created_at, rotated.created_at);
    state
        .with_connection(home.path(), |db| {
            save(db, store.as_ref(), alias, None, true, None)
        })
        .unwrap();
    assert_eq!(store.load(alias).unwrap().access, "brave-rotated-key");
    let oauth = OpenAiCodexState {
        manager: Arc::new(OAuthManager::production(store.clone())),
    };
    let (secret, models) = oauth
        .credential_and_models(&state, home.path(), alias)
        .unwrap();
    assert_eq!(secret.access, "brave-rotated-key");
    assert!(models.is_empty());
    store.fail_load(true);
    assert!(super::super::catalog_cache::list(&state, home.path()).unwrap()[0].models_available);
    assert!(oauth
        .manager
        .list_accounts(&state, home.path(), None)
        .unwrap()[0]
        .models
        .is_empty());
    state
        .with_connection(home.path(), |db| {
            db.execute(
                "UPDATE provider_accounts SET enabled=0 WHERE alias=?1",
                [alias],
            )
            .map_err(crate::persistence::PersistenceError::from)
        })
        .unwrap();
    assert_eq!(
        oauth
            .credential_and_models(&state, home.path(), alias)
            .err()
            .unwrap()
            .code,
        "account_disabled"
    );
    store.fail_load(false);
    state
        .with_connection(home.path(), |db| {
            crate::openai_codex::disconnect_provider_account(db, store.as_ref(), alias)
        })
        .unwrap();
    assert!(state
        .list_provider_accounts(home.path())
        .unwrap()
        .is_empty());
    assert!(store.load(alias).is_err());
    state.close();
}

#[test]
fn invalid_keys_aliases_duplicates_and_store_failures_preserve_existing_accounts() {
    let mut db = database();
    let store = InMemorySecretStore::default();
    let alias = "brave-search-work";
    for key in [
        None,
        Some(""),
        Some("has spaces"),
        Some("injected\r\nheader"),
    ] {
        assert!(save(&mut db, &store, alias, key, false, None).is_err());
    }
    for alias in ["brave-search-", "brave-search-invalid/", "custom-work"] {
        assert!(save(&mut db, &store, alias, Some("key"), false, None).is_err());
    }
    store.fail_store(true);
    assert!(save(&mut db, &store, alias, Some("key"), false, None).is_err());
    assert_eq!(
        db.query_row("SELECT count(*) FROM provider_accounts", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    store.fail_store(false);
    save(&mut db, &store, alias, Some("original"), false, None).unwrap();
    assert!(save(&mut db, &store, alias, Some("duplicate"), false, None).is_err());
    store.fail_store(true);
    assert!(save(
        &mut db,
        &store,
        alias,
        Some("replacement"),
        true,
        Some("New name")
    )
    .is_err());
    assert_eq!(store.load(alias).unwrap().access, "original");
    assert!(db
        .query_row(
            "SELECT display_name FROM provider_accounts WHERE alias=?1",
            [alias],
            |row| row.get::<_, Option<String>>(0)
        )
        .unwrap()
        .is_none());
}

#[test]
fn a_failed_commit_restores_the_previous_secure_key_and_metadata() {
    let mut db = database();
    let store = InMemorySecretStore::default();
    save(
        &mut db,
        &store,
        "brave-search-work",
        Some("original"),
        false,
        Some("Original"),
    )
    .unwrap();
    db.execute_batch("CREATE TABLE commit_guard(alias TEXT REFERENCES provider_accounts(alias) DEFERRABLE INITIALLY DEFERRED);
        CREATE TRIGGER fail_brave_update AFTER UPDATE ON provider_accounts BEGIN INSERT INTO commit_guard VALUES ('missing'); END;").unwrap();
    assert!(save(
        &mut db,
        &store,
        "brave-search-work",
        Some("replacement"),
        true,
        Some("Changed")
    )
    .is_err());
    assert_eq!(store.load("brave-search-work").unwrap().access, "original");
    assert_eq!(
        db.query_row("SELECT display_name FROM provider_accounts", [], |row| {
            row.get::<_, String>(0)
        })
        .unwrap(),
        "Original"
    );
}
