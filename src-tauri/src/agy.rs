//! The official AGY CLI owns login and inference; Jarvis supplies the scoped MCP bridge.
use serde::{Deserialize, Serialize};

mod metadata;
mod transport;
mod usage;

pub(crate) use metadata::{AgyState, RuntimeStatus};
pub(crate) use transport::{
    prepare_executor_workspace, private_directory as prepare_workspace, AgyProcess, RunOptions,
};

#[tauri::command]
pub(crate) async fn get_agy_runtime(
    state: tauri::State<'_, AgyState>,
    system: tauri::State<'_, crate::system::SystemState>,
) -> Result<RuntimeStatus, String> {
    let preferences = system.agy_preferences()?;
    let mut status = if preferences.enabled {
        metadata::cached(&state, false).await
    } else {
        metadata::local_status()
    };
    status.preferences = preferences;
    Ok(status)
}

#[tauri::command]
pub(crate) async fn refresh_agy_runtime(
    state: tauri::State<'_, AgyState>,
    system: tauri::State<'_, crate::system::SystemState>,
) -> Result<RuntimeStatus, String> {
    *state.usage.lock().await = usage::Cache::default();
    let mut status = metadata::cached(&state, true).await;
    status.preferences = system.agy_preferences()?;
    Ok(status)
}

#[tauri::command]
pub(crate) async fn get_agy_usage(
    state: tauri::State<'_, AgyState>,
    system: tauri::State<'_, crate::system::SystemState>,
) -> Result<crate::openai_codex::usage::AccountUsage, String> {
    if !system.agy_preferences()?.enabled {
        return Err("Ative o Antigravity CLI em Configurações → Provedores.".into());
    }
    Ok(usage::cached(&state).await)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub(crate) struct ProviderPreferences {
    pub enabled: bool,
    pub show_usage: bool,
    pub disabled_models: Vec<String>,
}

impl Default for ProviderPreferences {
    fn default() -> Self {
        Self {
            enabled: false,
            show_usage: true,
            disabled_models: Vec::new(),
        }
    }
}

impl ProviderPreferences {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.disabled_models.len() > 256 {
            return Err("Selecione até 256 modelos do Antigravity CLI.".into());
        }
        for model in &self.disabled_models {
            validate_selection(model, None)?;
        }
        Ok(())
    }

    pub(crate) fn allows(&self, model: &str) -> bool {
        self.enabled && !self.disabled_models.iter().any(|id| id == model)
    }
}

pub(crate) fn validate_available_model(home: &std::path::Path, model: &str) -> Result<(), String> {
    if !crate::system::backup_preferences(home)?.agy.allows(model) {
        return Err(
            "Disponibilize este modelo do Antigravity CLI em Configurações → Provedores.".into(),
        );
    }
    Ok(())
}

pub(crate) fn validate_selection(model: &str, reasoning: Option<&str>) -> Result<(), String> {
    if model.trim().is_empty()
        || model.len() > 256
        || model.starts_with('-')
        || model.chars().any(char::is_control)
    {
        return Err("Informe um modelo válido do Antigravity CLI.".into());
    }
    if reasoning.is_some_and(|value| !matches!(value, "low" | "medium" | "high" | "max")) {
        return Err("O esforço do Antigravity CLI deve ser low, medium, high ou max.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
