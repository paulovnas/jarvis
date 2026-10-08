//! Public account/model metadata only. Startup must not refresh credentials or
//! wait for the network lock just to render the accounts already configured.
use super::*;
use std::{collections::BTreeMap, io::Read, path::Path};

#[derive(Serialize, Deserialize)]
struct Entry {
    account_id: String,
    account: ProviderAccount,
}

fn path(home: &Path) -> std::path::PathBuf {
    crate::data_dir::root(home).join("provider-catalog.json")
}

fn load(home: &Path) -> BTreeMap<String, Entry> {
    std::fs::File::open(path(home))
        .ok()
        .and_then(|file| serde_json::from_reader(file.take(4 * 1024 * 1024)).ok())
        .unwrap_or_default()
}

pub(super) fn remember(
    home: &Path,
    records: &[ProviderAccountRecord],
    accounts: &[ProviderAccount],
) {
    let mut cached = load(home);
    cached.retain(|alias, entry| {
        records
            .iter()
            .any(|record| record.alias == *alias && record.account_id == entry.account_id)
    });
    for account in accounts
        .iter()
        .filter(|account| account.models_available && account.provider_kind != "custom")
    {
        if let Some(record) = records.iter().find(|record| record.alias == account.alias) {
            cached.insert(
                account.alias.clone(),
                Entry {
                    account_id: record.account_id.clone(),
                    account: account.clone(),
                },
            );
        }
    }
    // Cache failure must not turn a successful provider response into a failure.
    let save = || -> Result<(), Box<dyn std::error::Error>> {
        let destination = path(home);
        let mut temporary = tempfile::NamedTempFile::new_in(crate::data_dir::root(home))?;
        serde_json::to_writer(temporary.as_file_mut(), &cached)?;
        temporary.persist(destination)?;
        Ok(())
    };
    let _ = save();
}

pub(super) fn list(
    state: &persistence::AppState,
    home: &Path,
) -> Result<Vec<ProviderAccount>, ProviderError> {
    let mut cached = load(home);
    state
        .list_provider_accounts(home)
        .map_err(|_| ProviderError::database())?
        .into_iter()
        .map(|record| {
            if record.provider_kind == "opencode-go" {
                return attach_model_exclusions(
                    state,
                    home,
                    opencode_go::account(state, home, record)?,
                );
            }
            if record.provider_kind == "custom" {
                return attach_model_exclusions(state, home, custom::account(state, home, record)?);
            }
            let saved = cached.remove(&record.alias).filter(|entry| {
                entry.account_id == record.account_id
                    && entry.account.provider_kind == record.provider_kind
            });
            let mut account = ProviderAccount::from_record(record, None, Vec::new(), false);
            account.models_stale = account.enabled;
            if let Some(saved) = saved {
                account.email = saved.account.email;
                account.account_type = saved.account.account_type;
                if account.enabled {
                    account.models = saved.account.models;
                    account.models_available = saved.account.models_available;
                }
            }
            attach_model_exclusions(state, home, account)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_model_cache_defaults_to_normal_then_accepts_refreshed_fast_capability() {
        let home = tempfile::tempdir().unwrap();
        let state = persistence::AppState::default();
        let records = state.with_connection(home.path(), |db| {
            db.execute("INSERT INTO provider_accounts(alias,provider_kind,account_id) VALUES ('openai-codex-local','openai-codex','id')", []).unwrap();
            persistence::list_provider_accounts(db)
        }).unwrap();
        let model: ProviderModel = serde_json::from_value(serde_json::json!({
            "id":"model", "name":"Model", "reasoningLevels":[], "defaultReasoningLevel":null,
            "supportsFast":true
        }))
        .unwrap();
        let account = ProviderAccount::from_record(records[0].clone(), None, vec![model], true);
        remember(home.path(), &records, std::slice::from_ref(&account));
        let mut legacy: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path(home.path())).unwrap()).unwrap();
        legacy["openai-codex-local"]["account"]["models"][0]
            .as_object_mut()
            .unwrap()
            .remove("supportsFast");
        std::fs::write(path(home.path()), serde_json::to_vec(&legacy).unwrap()).unwrap();
        let cached = list(&state, home.path()).unwrap();
        assert!(cached[0].models_stale);
        assert!(!cached[0].models[0].supports_fast);
        remember(home.path(), &records, &[account]);
        assert!(list(&state, home.path()).unwrap()[0].models[0].supports_fast);
    }

    #[test]
    fn local_catalog_survives_failed_refresh_and_respects_current_account_settings() {
        let home = tempfile::tempdir().unwrap();
        let state = persistence::AppState::default();
        let records = state.with_connection(home.path(), |db| {
            db.execute("INSERT INTO provider_accounts(alias,provider_kind,account_id) VALUES ('openai-codex-local','openai-codex','id')", []).unwrap();
            persistence::list_provider_accounts(db)
        }).unwrap();
        let mut account = ProviderAccount::from_record(
            records[0].clone(),
            None,
            vec![ProviderModel {
                supports_fast: false,
                id: "test-model".into(),
                name: "Test model".into(),
                reasoning_levels: vec![],
                default_reasoning_level: None,
                multi_agent_reasoning_effort: None,
                context_window: None,
            }],
            true,
        );
        remember(home.path(), &records, &[account.clone()]);
        account.models_available = false;
        account.models.clear();
        remember(home.path(), &records, &[account]);
        let restored = list(&state, home.path()).unwrap();
        assert_eq!(restored[0].models[0].id, "test-model");
        assert!(restored[0].models_available);
        assert!(restored[0].models_stale);
        let manager = OAuthManager::production(std::sync::Arc::new(InMemorySecretStore::default()));
        let _network_operation = manager.credentials_guard.lock().unwrap();
        assert_eq!(
            manager.list_accounts(&state, home.path(), None).unwrap(),
            restored
        );

        state
            .with_connection(home.path(), |db| {
                db.execute("UPDATE provider_accounts SET enabled=0,show_usage=0", [])
                    .map_err(PersistenceError::from)
            })
            .unwrap();
        let disabled = list(&state, home.path()).unwrap();
        assert!(!disabled[0].enabled);
        assert!(!disabled[0].show_usage);
        assert!(disabled[0].models.is_empty());
        state
            .with_connection(home.path(), |db| {
                db.execute(
                    "UPDATE provider_accounts SET enabled=1,account_id='new-identity'",
                    [],
                )
                .map_err(PersistenceError::from)
            })
            .unwrap();
        let replaced = list(&state, home.path()).unwrap();
        assert!(replaced[0].models.is_empty());
        assert!(replaced[0].models_stale);
    }
}
