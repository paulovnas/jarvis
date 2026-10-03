//! Current public Go prices, independently of account credentials and inference.
use super::{client, public_json, BASE_URL};
use crate::openai_codex::ProviderError;
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::HashSet,
    sync::Mutex,
    time::{Duration, Instant},
};

const CATALOG_URL: &str = "https://models.opencode.ai/api.json";
const FRESH_FOR: Duration = Duration::from_secs(300);
const RETRY_AFTER: Duration = Duration::from_secs(15);
static CACHE: Mutex<Option<Cached>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FreeModel {
    pub id: String,
    pub name: String,
}

struct Cached {
    checked_at: Instant,
    result: Result<Vec<FreeModel>, ProviderError>,
}

fn unavailable() -> ProviderError {
    ProviderError::new(
        "opencode_go_free_models",
        "Não foi possível confirmar os modelos gratuitos do OpenCode Go. Tente novamente.",
    )
}

fn zero_prices(value: &Value, tier: bool, top_level: bool) -> bool {
    let Some(cost) = value.as_object() else {
        return false;
    };
    if cost.get("input").and_then(Value::as_f64) != Some(0.0)
        || cost.get("output").and_then(Value::as_f64) != Some(0.0)
        || (tier && !cost.contains_key("tier"))
    {
        return false;
    }
    cost.iter().all(|(key, value)| match key.as_str() {
        "input" | "output" | "cache_read" | "cache_write" => value.as_f64() == Some(0.0),
        "tiers" if top_level => value
            .as_array()
            .is_some_and(|tiers| tiers.iter().all(|value| zero_prices(value, true, false))),
        "context_over_200k" if top_level => zero_prices(value, false, false),
        "tier" if tier => value.as_object().is_some_and(|tier| {
            tier.len() == 2
                && tier.get("type").and_then(Value::as_str) == Some("context")
                && tier
                    .get("size")
                    .and_then(Value::as_f64)
                    .is_some_and(|size| size.is_finite() && size > 0.0)
        }),
        // A new price dimension needs an explicit interpretation before we can
        // promise that a model is free. Absence is never a zero price.
        _ => false,
    })
}

fn parse(ids: &Value, metadata: &Value) -> Result<Vec<FreeModel>, ProviderError> {
    let ids = ids["data"].as_array().ok_or_else(unavailable)?;
    let models = metadata["opencode-go"]["models"]
        .as_object()
        .ok_or_else(unavailable)?;
    let mut seen = HashSet::new();
    let mut free = Vec::new();
    for entry in ids {
        let Some(id) = entry["id"].as_str().filter(|id| {
            !id.is_empty()
                && id.len() <= 200
                && id.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/')
                })
        }) else {
            continue;
        };
        if !seen.insert(id) {
            continue;
        }
        let Some(model) = models.get(id) else {
            continue;
        };
        let Some(name) = model["name"].as_str().filter(|name| {
            !name.trim().is_empty() && name.len() <= 200 && !name.chars().any(char::is_control)
        }) else {
            continue;
        };
        if zero_prices(&model["cost"], false, true) {
            free.push(FreeModel {
                id: id.into(),
                name: name.into(),
            });
        }
    }
    free.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    Ok(free)
}

fn fetch_from(
    client: &reqwest::blocking::Client,
    availability_url: &str,
    catalog_url: &str,
) -> Result<Vec<FreeModel>, ProviderError> {
    parse(
        &public_json(client, availability_url)?,
        &public_json(client, catalog_url)?,
    )
}

fn cached_models(
    cache: &Mutex<Option<Cached>>,
    fetch: impl FnOnce() -> Result<Vec<FreeModel>, ProviderError>,
) -> Result<Vec<FreeModel>, ProviderError> {
    // This private lock only coalesces public catalog reads, on a blocking worker.
    // It never holds up credentials, running chats or other provider catalogs.
    let mut cache = cache.lock().map_err(|_| unavailable())?;
    if let Some(cached) = &*cache {
        let ttl = if cached.result.is_ok() {
            FRESH_FOR
        } else {
            RETRY_AFTER
        };
        if cached.checked_at.elapsed() < ttl {
            return cached.result.clone();
        }
    }
    let result = fetch();
    // A failed refresh replaces the expired list rather than advertising an
    // outdated price as free. Briefly cache errors to coalesce failed requests.
    *cache = Some(Cached {
        checked_at: Instant::now(),
        result: result.clone(),
    });
    result
}

#[tauri::command]
pub async fn get_opencode_go_free_models() -> Result<Vec<FreeModel>, ProviderError> {
    tauri::async_runtime::spawn_blocking(|| {
        cached_models(&CACHE, || {
            fetch_from(&client()?, &format!("{BASE_URL}/models"), CATALOG_URL)
        })
    })
    .await
    .map_err(|_| unavailable())?
}

#[cfg(test)]
mod tests;
