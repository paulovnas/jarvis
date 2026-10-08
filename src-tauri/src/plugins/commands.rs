//! Native review receipts never deserialize an executable package from the UI.
use super::{apply, error, preview, Catalog, Operation, Prepared, Preview, Result};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};

const MAX_RECEIPTS: usize = 16;
const RECEIPT_LIFETIME: Duration = Duration::from_secs(30 * 60);

struct Receipt {
    revision: u64,
    prepared: Prepared,
    created: Instant,
}

#[derive(Clone, Default)]
pub(crate) struct PluginsState(Arc<Mutex<HashMap<String, Receipt>>>);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewReceipt {
    receipt_id: String,
    revision: u64,
    preview: Preview,
}

impl PluginsState {
    fn insert(&self, revision: u64, prepared: Prepared) -> Result<PreviewReceipt> {
        if prepared.revision() != revision {
            return Err(error(
                "stale_plugin_preview",
                "A revisão do pacote mudou. Prepare a alteração novamente.",
            ));
        }
        let mut receipts = self
            .0
            .lock()
            .map_err(|_| error("plugins_storage", "Não foi possível preparar a revisão."))?;
        receipts.retain(|_, receipt| receipt.created.elapsed() < RECEIPT_LIFETIME);
        if receipts.len() >= MAX_RECEIPTS {
            return Err(error(
                "plugins_preview_limit",
                "Feche uma revisão de plugin antes de abrir outra.",
            ));
        }
        let receipt_id = crate::library::new_id()
            .map_err(|_| error("plugins_storage", "Não foi possível identificar a revisão."))?;
        let result = PreviewReceipt {
            receipt_id: receipt_id.clone(),
            revision,
            preview: prepared.preview.clone(),
        };
        receipts.insert(
            receipt_id,
            Receipt {
                revision,
                prepared,
                created: Instant::now(),
            },
        );
        Ok(result)
    }

    fn take(&self, id: &str, revision: u64) -> Result<Prepared> {
        let receipt = self
            .0
            .lock()
            .map_err(|_| error("plugins_storage", "Não foi possível acessar a revisão."))?
            .remove(id)
            .ok_or_else(|| {
                error(
                    "stale_plugin_preview",
                    "Esta revisão não está mais disponível. Prepare a alteração novamente.",
                )
            })?;
        if receipt.revision != revision || receipt.created.elapsed() >= RECEIPT_LIFETIME {
            return Err(error(
                "stale_plugin_preview",
                "Esta revisão expirou ou mudou. Prepare a alteração novamente.",
            ));
        }
        Ok(receipt.prepared)
    }
}

pub(crate) fn emit_changed(app: &tauri::AppHandle) {
    for event in [
        "plugins:changed",
        "skills:changed",
        "mcp-servers:changed",
        "hooks:changed",
    ] {
        let _ = app.emit(event, ());
    }
}

#[tauri::command]
pub(crate) async fn list_plugins(app: tauri::AppHandle) -> Result<Catalog> {
    let home = app.path().home_dir().map_err(|_| {
        error(
            "plugins_storage",
            "Não foi possível localizar a pasta do usuário.",
        )
    })?;
    let discovery_issues = super::discover_builtin_catalogs(&home).await;
    tauri::async_runtime::spawn_blocking(move || {
        let mut catalog = super::catalog_with_icons(&home)?;
        catalog.issues.extend(discovery_issues);
        Ok(catalog)
    })
    .await
    .map_err(|_| error("plugins_storage", "Não foi possível carregar os plugins."))?
}

#[tauri::command]
pub(crate) async fn preview_plugin_change(
    app: tauri::AppHandle,
    plugins: tauri::State<'_, PluginsState>,
    operation: Operation,
    expected_revision: u64,
) -> Result<PreviewReceipt> {
    let home = app.path().home_dir().map_err(|_| {
        error(
            "plugins_storage",
            "Não foi possível localizar a pasta do usuário.",
        )
    })?;
    let prepared = preview(&home, expected_revision, operation).await?;
    plugins.insert(expected_revision, prepared)
}

#[tauri::command]
pub(crate) async fn apply_plugin_change(
    app: tauri::AppHandle,
    plugins: tauri::State<'_, PluginsState>,
    receipt_id: String,
    expected_revision: u64,
) -> Result<Catalog> {
    let prepared = plugins.take(&receipt_id, expected_revision)?;
    let home = app.path().home_dir().map_err(|_| {
        error(
            "plugins_storage",
            "Não foi possível localizar a pasta do usuário.",
        )
    })?;
    let saved = tauri::async_runtime::spawn_blocking(move || {
        apply(&home, &prepared)?;
        super::catalog_with_icons(&home)
    })
    .await
    .map_err(|_| error("plugins_storage", "Não foi possível aplicar a revisão."))??;
    emit_changed(&app);
    Ok(saved)
}

#[tauri::command]
pub(crate) fn cancel_plugin_change(
    plugins: tauri::State<'_, PluginsState>,
    receipt_id: String,
) -> Result<()> {
    plugins
        .0
        .lock()
        .map_err(|_| error("plugins_storage", "Não foi possível fechar a revisão."))?
        .remove(&receipt_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::catalog;

    #[tokio::test]
    async fn native_receipt_is_one_shot_and_bound_to_revision() {
        let home = tempfile::tempdir().unwrap();
        let registry = PluginsState::default();
        let revision = catalog(home.path()).unwrap().revision;
        let operation: Operation = serde_json::from_value(serde_json::json!({"action":"create","draft":{"name":"receipt-fixture","description":"Teste de revisão","skills":[],"mcpServers":{},"hooks":null,"apps":{},"files":[]}})).unwrap();
        let prepared = preview(home.path(), revision, operation).await.unwrap();
        let receipt = registry.insert(revision, prepared.clone()).unwrap();
        assert!(catalog(home.path()).unwrap().installed.is_empty());
        let taken = registry.take(&receipt.receipt_id, revision).unwrap();
        assert!(registry.take(&receipt.receipt_id, revision).is_err());
        assert_eq!(apply(home.path(), &taken).unwrap().installed.len(), 1);
        let stale = registry.insert(revision, prepared.clone()).unwrap();
        assert!(registry.take(&stale.receipt_id, revision + 1).is_err());
        assert!(registry.take(&stale.receipt_id, revision).is_err());
        let expired = registry.insert(revision, prepared.clone()).unwrap();
        registry
            .0
            .lock()
            .unwrap()
            .get_mut(&expired.receipt_id)
            .unwrap()
            .created = Instant::now() - RECEIPT_LIFETIME;
        assert!(registry.take(&expired.receipt_id, revision).is_err());
        for _ in 0..MAX_RECEIPTS {
            registry.insert(revision, prepared.clone()).unwrap();
        }
        assert_eq!(
            registry.insert(revision, prepared).unwrap_err().code,
            "plugins_preview_limit"
        );
        assert!(registry.take("invented", revision).is_err());
    }
}
