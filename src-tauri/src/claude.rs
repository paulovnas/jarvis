//! Claude owns authentication and inference; Jarvis owns the scoped process and tool bridge.
use serde::{Deserialize, Serialize};

mod metadata;
mod transport;
mod usage;

pub(crate) use metadata::{ClaudeState, RuntimeStatus};
pub(crate) use transport::{ClaudeProcess, RunOptions};

#[tauri::command]
pub(crate) async fn get_claude_runtime(
    state: tauri::State<'_, ClaudeState>,
    system: tauri::State<'_, crate::system::SystemState>,
) -> Result<RuntimeStatus, String> {
    let mut status = metadata::cached(&state, false).await;
    status.preferences = system.claude_preferences()?;
    Ok(status)
}

#[tauri::command]
pub(crate) async fn refresh_claude_runtime(
    state: tauri::State<'_, ClaudeState>,
    system: tauri::State<'_, crate::system::SystemState>,
) -> Result<RuntimeStatus, String> {
    *state.usage.lock().await = usage::Cache::default();
    let mut status = metadata::cached(&state, true).await;
    status.preferences = system.claude_preferences()?;
    Ok(status)
}

#[tauri::command]
pub(crate) async fn get_claude_usage(
    state: tauri::State<'_, ClaudeState>,
    system: tauri::State<'_, crate::system::SystemState>,
) -> Result<crate::openai_codex::usage::AccountUsage, String> {
    if !system.claude_preferences()?.enabled {
        return Err("Ative o Claude Code em Configurações → Provedores.".into());
    }
    Ok(usage::cached(&state).await)
}

/// One local CLI installation, independent of API account credentials.
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
            enabled: true,
            show_usage: true,
            disabled_models: Vec::new(),
        }
    }
}

impl ProviderPreferences {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.disabled_models.len() > 256 {
            return Err("Selecione até 256 modelos do Claude Code.".into());
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
    let preferences = crate::system::backup_preferences(home)?.claude;
    if !preferences.enabled {
        return Err("Ative o Claude Code em Configurações → Provedores.".into());
    }
    if !preferences.allows(model) {
        return Err(
            "Disponibilize este modelo do Claude Code em Configurações → Provedores.".into(),
        );
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub(crate) enum Executor {
    #[default]
    Jarvis,
    Claude,
}

impl Executor {
    pub(crate) fn is_jarvis(&self) -> bool {
        *self == Self::Jarvis
    }
}

pub(crate) fn validate_selection(model: &str, reasoning: Option<&str>) -> Result<(), String> {
    if model.trim().is_empty()
        || model.len() > 256
        || model.starts_with('-')
        || model.chars().any(char::is_control)
    {
        return Err("Informe um modelo válido do Claude Code.".into());
    }
    if reasoning.is_some_and(|value| !matches!(value, "low" | "medium" | "high" | "xhigh" | "max"))
    {
        return Err("O esforço do Claude deve ser low, medium, high, xhigh ou max.".into());
    }
    Ok(())
}

pub(crate) fn new_session_id() -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| "Não foi possível identificar a sessão Claude.".to_string())?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}

#[cfg(test)]
mod tests;
