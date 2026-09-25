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
        if !force && !current.needs_refresh()? {
            return Ok(current.clone());
        }
        let state = self.state.clone();
        let oauth = self.oauth.clone();
        let home = self.home.clone();
        let alias = self.alias.clone();
        let previous = current.clone();
        let refresh = tauri::async_runtime::spawn_blocking(move || {
            oauth.renew_inference_credential(&state, &home, &alias, &previous, force)
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
