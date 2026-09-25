//! Renew a running inference without rediscovering models or switching accounts.
use super::*;

impl CodexCredential {
    pub(crate) fn needs_refresh(&self) -> Result<bool, ProviderError> {
        Ok(self.custom.is_none() && self.expires <= current_time_millis()?.saturating_add(60_000))
    }
}

impl OpenAiCodexState {
    pub(crate) fn renew_inference_credential(
        &self,
        state: &persistence::AppState,
        home: &std::path::Path,
        alias: &str,
        current: &CodexCredential,
        force: bool,
    ) -> Result<CodexCredential, ProviderError> {
        if current.custom.is_some() {
            return Ok(current.clone());
        }
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
        let mut stored = self.manager.secret_store.load(alias).map_err(|_| {
            ProviderError::new(
                "credential_missing",
                "Reconecte a conta nas configurações para enviar mensagens.",
            )
        })?;
        if record.account_id != current.account_id
            || stored.account_id != current.account_id
            || stored.project_id != current.project_id
            || record.provider_kind == "custom"
            || (record.provider_kind == "antigravity") != current.project_id.is_some()
        {
            return Err(ProviderError::new(
                "account_mismatch",
                "A conta mudou durante a execução. Envie uma nova mensagem com a conta desejada.",
            ));
        }
        // A quota probe or another worker may already have rotated this token.
        if stored.needs_refresh()? || (force && stored.access == current.access) {
            stored = refresh_credential(&self.manager.endpoints, &stored)?;
            if stored.account_id != current.account_id || stored.project_id != current.project_id {
                return Err(ProviderError::new("account_mismatch", "A renovação retornou outra conta. Reconecte a conta desejada nas configurações."));
            }
            self.manager
                .secret_store
                .store(alias, &stored)
                .map_err(|_| ProviderError::internal())?;
        }
        // Keep the selected model's metadata and endpoint fixed for this turn.
        let mut renewed = current.clone();
        renewed.access = stored.access;
        renewed.refresh = stored.refresh;
        renewed.expires = stored.expires;
        Ok(renewed)
    }
}
