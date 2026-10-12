//! The turn keeps its model/transport, but OAuth tokens must outlive neither expiry nor a 401.
use super::*;
use crate::{openai_codex::OpenAiCodexState, persistence::AppState};
use std::path::PathBuf;

pub(super) struct Authentication {
    state: AppState,
    oauth: OpenAiCodexState,
    home: PathBuf,
    alias: String,
    current: tokio::sync::Mutex<CodexCredential>,
}

impl Authentication {
    pub(super) fn new(
        state: AppState,
        oauth: OpenAiCodexState,
        home: PathBuf,
        alias: String,
        credential: CodexCredential,
    ) -> Self {
        Self {
            state,
            oauth,
            home,
            alias,
            current: tokio::sync::Mutex::new(credential),
        }
    }

    pub(super) async fn credential(
        &self,
        force: bool,
        mut signal: watch::Receiver<bool>,
    ) -> Result<CodexCredential, AgentError> {
        if *signal.borrow() {
            return Err(AgentError::cancelled());
        }
        let mut current = tokio::select! {
            _ = cancelled(&mut signal) => return Err(AgentError::cancelled()),
            current = self.current.lock() => current,
        };
        let cached = !force && !current.needs_refresh()?;
        let state = self.state.clone();
        let oauth = self.oauth.clone();
        let home = self.home.clone();
        let alias = self.alias.clone();
        let previous = current.clone();
        let refresh = tauri::async_runtime::spawn_blocking(move || {
            if cached {
                // Cached tokens avoid Keychain/model discovery, but never outlive
                // the enabled account identity to which this turn was bound.
                if previous.custom.is_none() {
                    let records = state
                        .list_provider_accounts(&home)
                        .map_err(crate::openai_codex::ProviderError::from)?;
                    let record = records
                        .iter()
                        .find(|record| record.alias == alias)
                        .ok_or_else(|| {
                            crate::openai_codex::ProviderError::new(
                                "account_missing",
                                "A conta selecionada foi desconectada.",
                            )
                        })?;
                    if !record.enabled {
                        return Err(crate::openai_codex::ProviderError::new(
                            "account_disabled",
                            "Ative a conta nas configurações para usá-la.",
                        ));
                    }
                    if record.account_id != previous.account_id
                        || record.provider_kind == "custom"
                        || (record.provider_kind == "antigravity") != previous.project_id.is_some()
                    {
                        return Err(crate::openai_codex::ProviderError::new("account_mismatch", "A conta mudou durante a execução. Envie uma nova mensagem com a conta desejada."));
                    }
                }
                Ok(previous)
            } else {
                oauth.renew_inference_credential(&state, &home, &alias, &previous, force)
            }
        });
        *current = tokio::select! {
            _ = cancelled(&mut signal) => return Err(AgentError::cancelled()),
            result = refresh => result.map_err(|_| AgentError::internal())??,
        };
        Ok(current.clone())
    }
}

pub(super) fn unauthorized(error: &AgentError) -> bool {
    error.code == "provider_auth"
        && error
            .provider_metadata
            .as_ref()
            .is_some_and(|metadata| metadata.http_status == Some(401))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cached_tokens_stop_when_the_bound_account_is_removed_disabled_or_replaced() {
        for (change, expected) in [
            ("removed", "account_missing"),
            ("disabled", "account_disabled"),
            ("replaced", "account_mismatch"),
        ] {
            let home = tempfile::tempdir().unwrap();
            let state = AppState::default();
            let alias = "openai-codex-test";
            state
                .with_connection(home.path(), |db| {
                    crate::persistence::insert_provider_account(db, alias, "original")?;
                    Ok::<_, crate::persistence::PersistenceError>(())
                })
                .unwrap();
            let authentication = Authentication::new(
                state.clone(),
                OpenAiCodexState::default(),
                home.path().to_path_buf(),
                alias.into(),
                CodexCredential::new(
                    "synthetic-access",
                    "synthetic-refresh",
                    i64::MAX,
                    "original",
                    None,
                    None,
                ),
            );
            let (_stop, signal) = watch::channel(false);
            assert!(authentication
                .credential(false, signal.clone())
                .await
                .is_ok());
            state.with_connection(home.path(), |db| {
                match change {
                    "removed" => crate::persistence::delete_provider_account(db, alias)?,
                    "disabled" => { db.execute("UPDATE provider_accounts SET enabled=0 WHERE alias=?1", [alias])?; }
                    "replaced" => { db.execute("UPDATE provider_accounts SET account_id='replacement' WHERE alias=?1", [alias])?; }
                    _ => unreachable!(),
                }
                Ok::<_, crate::persistence::PersistenceError>(())
            }).unwrap();
            let result = authentication.credential(false, signal).await;
            assert_eq!(
                result
                    .err()
                    .expect("cached identity must be revalidated")
                    .code,
                expected
            );
            state.close();
        }
    }
}
