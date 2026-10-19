//! Public image metadata is separate from conversational model catalogs and credentials.
use super::{custom, ProviderAccount, ProviderError, ProviderModel};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{io::Read, path::Path, time::Duration};
use tauri::Manager;

pub(crate) const ANTIGRAVITY_MODEL: &str = "gemini-3.1-flash-image";
pub(crate) const CODEX_MODEL: &str = "gpt-image-2";
const MAX_CATALOG: usize = 4 * 1024 * 1024;

#[derive(Deserialize, Serialize)]
struct ImageModel {
    #[serde(flatten)]
    model: ProviderModel,
    aspect_ratios: Vec<String>,
    min_references: usize,
    max_references: usize,
}

fn error(message: &'static str) -> ProviderError {
    ProviderError::new("image_catalog", message)
}

fn model(id: &str, name: &str) -> ProviderModel {
    ProviderModel {
        id: id.into(),
        name: name.into(),
        supports_fast: false,
        reasoning_levels: vec![],
        default_reasoning_level: None,
        multi_agent_reasoning_effort: None,
        context_window: None,
    }
}

pub(crate) fn is_openrouter(config: &custom::Config) -> bool {
    config.auth_mode == custom::AuthMode::Bearer
        && custom::discovery::validate_target(&config.base_url, "image").is_ok()
}

fn path(home: &Path) -> std::path::PathBuf {
    crate::data_dir::root(home).join("image-model-catalog.json")
}

fn cached(home: &Path) -> Vec<ImageModel> {
    std::fs::File::open(path(home))
        .ok()
        .and_then(|file| serde_json::from_reader(file.take(MAX_CATALOG as u64)).ok())
        .unwrap_or_default()
}

pub(super) fn for_account(account: &ProviderAccount) -> Vec<ProviderModel> {
    if !account.enabled {
        return vec![];
    }
    let mut models = match account.provider_kind.as_str() {
        "antigravity" => vec![model(ANTIGRAVITY_MODEL, "Gemini 3.1 Flash Image")],
        "openai-codex" => vec![model(CODEX_MODEL, "GPT Image 2")],
        "custom" | "openrouter" if account.custom.as_ref().is_some_and(is_openrouter) => account
            .custom
            .as_ref()
            .into_iter()
            .flat_map(|config| &config.models)
            .filter(|item| item.supports_image_generation)
            .map(|item| model(&item.id, &item.name))
            .collect(),
        _ => vec![],
    };
    models.retain(|model| !account.disabled_models.contains(&model.id));
    models
}

pub(crate) async fn lookup(home: &Path, id: &str) -> Result<custom::Model, ProviderError> {
    refresh(home).await?;
    let item = cached(home).into_iter().find(|item| item.model.id == id)
        .ok_or_else(|| ProviderError::new("model_discovery", "Modelo não encontrado nos catálogos de chat e de imagens do OpenRouter. Copie o ID completo ou configure manualmente."))?;
    Ok(custom::Model {
        id: item.model.id,
        name: item.model.name,
        context_window: 0,
        max_output_tokens: 0,
        supports_chat: false,
        supports_image_generation: true,
        supports_images: item.max_references > 0,
        supports_tools: false,
        reasoning: custom::Reasoning::None,
        reasoning_levels: vec![],
        default_reasoning_level: None,
        thinking_budget: None,
    })
}

pub(crate) fn available(
    state: &crate::persistence::AppState,
    home: &Path,
    alias: &str,
) -> Result<Vec<ProviderModel>, ProviderError> {
    Ok(super::catalog_cache::list(state, home)?
        .into_iter()
        .find(|account| account.alias == alias)
        .map_or_else(Vec::new, |account| account.image_models))
}

fn parse(value: &Value) -> Result<Vec<ImageModel>, ProviderError> {
    let entries = value["data"]
        .as_array()
        .filter(|entries| entries.len() <= 1000)
        .ok_or_else(|| {
            error("O catálogo de imagens do OpenRouter retornou um formato inválido.")
        })?;
    let mut models = vec![];
    for item in entries {
        let Some(id) = item["id"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 200 && !id.chars().any(char::is_control))
        else {
            continue;
        };
        let Some(name) = item["name"].as_str().filter(|name| {
            !name.trim().is_empty() && name.len() <= 240 && !name.chars().any(char::is_control)
        }) else {
            continue;
        };
        let input = item["architecture"]["input_modalities"].as_array();
        let output = item["architecture"]["output_modalities"].as_array();
        if !input.is_some_and(|values| values.contains(&Value::from("text")))
            || !output.is_some_and(|values| values.contains(&Value::from("image")))
        {
            continue;
        }
        let params = &item["supported_parameters"];
        let formats = params["output_format"]["values"]
            .as_array()
            .or_else(|| params["format"]["values"].as_array());
        if formats.is_some_and(|values| {
            !values
                .iter()
                .any(|value| matches!(value.as_str(), Some("png" | "jpeg" | "jpg" | "webp")))
        }) {
            continue;
        }
        let references = &params["input_references"];
        let max_references = if input.is_some_and(|values| values.contains(&Value::from("image"))) {
            references["max"].as_u64().unwrap_or(0).min(4) as usize
        } else {
            0
        };
        if params["n"]["min"].as_u64().unwrap_or(1) > 1
            || params["n"]["max"]
                .as_u64()
                .is_some_and(|maximum| maximum == 0)
            || references["min"].as_u64().unwrap_or(0) > 4
            || references["min"].as_u64().unwrap_or(0) > max_references as u64
        {
            continue;
        }
        if models.iter().any(|entry: &ImageModel| entry.model.id == id) {
            continue;
        }
        models.push(ImageModel {
            model: model(id, name),
            aspect_ratios: params["aspect_ratio"]["values"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .take(64)
                .map(str::to_owned)
                .collect(),
            min_references: references["min"].as_u64().unwrap_or(0) as usize,
            max_references,
        });
    }
    if models.is_empty() {
        return Err(error(
            "Nenhum modelo de imagens compatível foi encontrado no OpenRouter.",
        ));
    }
    Ok(models)
}

pub(crate) fn validate_request(
    home: &Path,
    model: &str,
    ratio: Option<&str>,
    references: usize,
) -> Result<(), ProviderError> {
    let selected = cached(home)
        .into_iter()
        .find(|item| item.model.id == model)
        .ok_or_else(|| {
            error("Atualize o catálogo de imagens e selecione um modelo OpenRouter disponível.")
        })?;
    if references < selected.min_references || references > selected.max_references {
        return Err(error("O modelo de imagens não aceita esta quantidade de referências. Selecione um modelo compatível."));
    }
    if ratio.is_some_and(|ratio| !selected.aspect_ratios.iter().any(|value| value == ratio)) {
        return Err(error("O modelo de imagens não aceita a proporção solicitada. Revise a proporção ou selecione outro modelo."));
    }
    Ok(())
}

async fn refresh(home: &Path) -> Result<Vec<ProviderModel>, ProviderError> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| ProviderError::internal())?;
    let mut response = client.get("https://openrouter.ai/api/v1/images/models").send().await
        .map_err(|_| error("Não foi possível atualizar os modelos de imagens do OpenRouter. O catálogo anterior foi mantido."))?;
    if !response.status().is_success() {
        return Err(error(
            "O OpenRouter não disponibilizou seu catálogo de imagens. Tente atualizar novamente.",
        ));
    }
    let mut bytes = vec![];
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| error("A consulta ao catálogo de imagens foi interrompida."))?
    {
        if bytes.len() + chunk.len() > MAX_CATALOG {
            return Err(error("O catálogo de imagens excedeu o limite de tamanho."));
        }
        bytes.extend_from_slice(&chunk);
    }
    let models = parse(
        &serde_json::from_slice(&bytes)
            .map_err(|_| error("O catálogo de imagens retornou dados inválidos."))?,
    )?;
    let values = models.iter().map(|item| item.model.clone()).collect();
    let root = crate::data_dir::root(home);
    std::fs::create_dir_all(&root).map_err(|_| ProviderError::database())?;
    let mut file = tempfile::NamedTempFile::new_in(root).map_err(|_| ProviderError::database())?;
    serde_json::to_writer(file.as_file_mut(), &models).map_err(|_| ProviderError::database())?;
    file.as_file()
        .sync_all()
        .map_err(|_| ProviderError::database())?;
    file.persist(path(home))
        .map_err(|_| ProviderError::database())?;
    Ok(values)
}

#[tauri::command]
pub async fn refresh_image_generation_models(
    app: tauri::AppHandle,
) -> Result<Vec<ProviderModel>, ProviderError> {
    let home = app
        .path()
        .home_dir()
        .map_err(|_| ProviderError::database())?;
    refresh(&home).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    #[ignore = "Reads only public OpenRouter image metadata and writes to a temporary directory"]
    async fn live_openrouter_image_catalog_includes_seedream_5_variants() {
        let home = tempfile::tempdir().unwrap();
        let models = refresh(home.path()).await.unwrap();
        for variant in ["flash", "lite", "pro"] {
            assert!(models
                .iter()
                .any(|model| model.id == format!("bytedance-seed/seedream-5-0-{variant}")));
        }
        assert!(!models.iter().any(|model| model.id.contains("vector")));
        validate_request(
            home.path(),
            "bytedance-seed/seedream-5-0-lite",
            Some("16:9"),
            1,
        )
        .unwrap();
    }

    #[test]
    fn parses_image_catalog_without_treating_vision_as_generation() {
        let image = json!({"id":"bytedance-seed/seedream-5-0-lite","name":"Seedream 5.0 Lite","architecture":{"input_modalities":["text","image"],"output_modalities":["image"]},"supported_parameters":{"n":{"min":1,"max":4},"input_references":{"min":0,"max":14},"aspect_ratio":{"values":["1:1","16:9"]}}});
        let mut text = image.clone();
        text["id"] = json!("vision-only");
        text["architecture"]["output_modalities"] = json!(["text"]);
        let mut vector = image.clone();
        vector["id"] = json!("vector");
        vector["supported_parameters"]["output_format"] = json!({"values":["svg"]});
        let values = parse(&json!({"data":[image.clone(),text,vector,image]})).unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].model.id, "bytedance-seed/seedream-5-0-lite");
        assert_eq!(values[0].max_references, 4);
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(crate::data_dir::root(home.path())).unwrap();
        std::fs::write(path(home.path()), serde_json::to_vec(&values).unwrap()).unwrap();
        assert!(validate_request(home.path(), &values[0].model.id, Some("16:9"), 1).is_ok());
        assert!(validate_request(home.path(), &values[0].model.id, Some("21:9"), 1).is_err());
        assert!(validate_request(home.path(), &values[0].model.id, None, 5).is_err());
        assert!(validate_request(home.path(), "missing", None, 0).is_err());
    }
}
