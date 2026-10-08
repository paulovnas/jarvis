//! Provider configuration stays in the native vault; the UI sees presence only.
use super::{error, installed, pip, python_path, ComponentId, CoreError};
use crate::mcp::{Keychain, Secrets};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::Path,
};
use tauri::Manager;

pub(super) const CREDENTIAL_KEYS: &str = "ARK_API_KEY ARK_BASE_URL ARK_CNY_PER_USD ARK_SEEDANCE_MODEL ATLASCLOUD_API_KEY AZURE_SPEECH_ENDPOINT AZURE_SPEECH_KEY AZURE_SPEECH_REGION AZURE_TTS_ENDPOINT BFL_API_KEY BLENDER_PATH CARTESIA_API_KEY COMFYUI_SERVER_URL COMFYUI_VIDEO_SERVER_URL COVERR_API_KEY DASHSCOPE_API_KEY DOUBAO_SPEECH_API_KEY DOUBAO_SPEECH_VOICE_TYPE ELEVENLABS_API_KEY FAL_AI_API_KEY FAL_KEY FISH_AUDIO_API_KEY FREESOUND_API_KEY GCLOUD_PROJECT GEMINI_API_KEY GOOGLE_API_KEY GOOGLE_APPLICATION_CREDENTIALS GOOGLE_CLOUD_LOCATION GOOGLE_CLOUD_PROJECT GOOGLE_CLOUD_PROJECT_ID GOOGLE_GENAI_USE_ENTERPRISE GOOGLE_GENAI_USE_VERTEXAI GOOGLE_TTS_API_KEY HEYGEN_API_KEY HF_TOKEN HIGGSFIELD_API_KEY HIGGSFIELD_API_SECRET HIGGSFIELD_KEY IDEOGRAM_API_KEY INWORLD_API_KEY KLING_API_BASE_URL KLING_API_KEY LTX_API_KEY MINIMAX_API_KEY MINIMAX_BASE_URL MINIMAX_REGION MODAL_LTX2_ENDPOINT_URL MUSIC_LIBRARY_DIR NARA_API_KEY OPENAI_API_KEY PEXELS_API_KEY PIPER_MODEL_PATH PIXABAY_API_KEY POND5_API_KEY QWEN_IMAGE_21_PATH REPLICATE_API_TOKEN RUNWAY_API_KEY RUNWAYML_API_SECRET SADTALKER_PATH SUNO_API_KEY TENCENT_TOKENHUB_API_KEY UNSPLASH_ACCESS_KEY VIDEO_GEN_LOCAL_ENABLED VIDEO_GEN_LOCAL_MODEL VIDEVO_API_KEY VOLC_ACCESSKEY VOLC_SECRETKEY WAV2LIP_PATH XAI_API_KEY";

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    allow_paid_tools: bool,
    allow_model_downloads: bool,
    credential_ref: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialField {
    pub key: String,
    pub label: String,
    pub configured: bool,
    pub secret: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionalPackage {
    pub id: String,
    pub label: String,
    pub installed: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Configuration {
    pub allow_paid_tools: bool,
    pub allow_model_downloads: bool,
    pub credentials: Vec<CredentialField>,
    pub optional_packages: Vec<OptionalPackage>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigurationInput {
    pub allow_paid_tools: bool,
    pub allow_model_downloads: bool,
    #[serde(default)]
    pub credentials: BTreeMap<String, String>,
    #[serde(default)]
    pub remove_credentials: Vec<String>,
}

const OPTIONAL: &[(&str, &str, &[&str])] = &[
    (
        "piper",
        "Motor Piper (configure uma voz ONNX e seu JSON)",
        &["piper-tts"],
    ),
    (
        "analysis",
        "Análise de vídeos, transcrição e cortes",
        &[
            "yt-dlp",
            "youtube-transcript-api",
            "faster-whisper",
            "scenedetect[opencv]",
            "pygments",
            "pydub",
            "beautifulsoup4",
        ],
    ),
    (
        "local-media",
        "Geração local de imagens, vídeo e música",
        &[
            "diffusers",
            "transformers",
            "accelerate",
            "torch",
            "torchaudio",
            "torchvision",
            "sentencepiece",
            "safetensors",
            "opencv-python",
            "imageio",
            "imageio-ffmpeg",
            "soundfile",
            "librosa",
            "rembg",
            "onnxruntime",
            "mediapipe",
        ],
    ),
    (
        "gpu",
        "Runtime GPU PyTorch (download grande)",
        &["torch", "torchaudio", "torchvision"],
    ),
    (
        "enhancement",
        "Restauração e ampliação local (modelos à parte)",
        &["gfpgan", "realesrgan", "torch", "torchvision"],
    ),
    (
        "animation",
        "Animações Manim (Cairo e Pango incluídos)",
        &["manim"],
    ),
];

fn path(home: &Path) -> std::path::PathBuf {
    super::super::root(home).join("openmontage/settings.json")
}
fn settings(home: &Path) -> Result<Settings, CoreError> {
    match fs::read(path(home)) {
        Ok(bytes) => {
            let settings: Settings = serde_json::from_slice(&bytes)
                .map_err(|_| error("Configuração do OpenMontage inválida."))?;
            if settings
                .credential_ref
                .as_ref()
                .is_some_and(|key| !key.starts_with("jarvis-core-openmontage-"))
            {
                return Err(error("Referência de credenciais inválida."));
            }
            Ok(settings)
        }
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(cause) => Err(cause.into()),
    }
}
fn values(
    settings: &Settings,
    secrets: &dyn Secrets,
) -> Result<BTreeMap<String, String>, CoreError> {
    let Some(reference) = &settings.credential_ref else {
        return Ok(BTreeMap::new());
    };
    let value = secrets.load(reference).map_err(|_| {
        error("As credenciais do OpenMontage não estão acessíveis no armazenamento seguro.")
    })?;
    let values: BTreeMap<String, String> =
        serde_json::from_str(&value).map_err(|_| error("Credenciais do OpenMontage inválidas."))?;
    if values.keys().any(|key| !known_key(key)) {
        return Err(error("Credencial desconhecida na configuração de vídeo."));
    }
    Ok(values)
}
fn known_key(key: &str) -> bool {
    CREDENTIAL_KEYS
        .split_whitespace()
        .any(|candidate| candidate == key)
}
fn validate_input(input: &ConfigurationInput) -> Result<(), CoreError> {
    if input.credentials.len() > 128
        || input.remove_credentials.len() > 128
        || input
            .credentials
            .iter()
            .any(|(key, value)| !known_key(key) || value.len() > 16_384 || value.contains('\0'))
        || input.remove_credentials.iter().any(|key| !known_key(key))
    {
        return Err(error(
            "Configuração inválida. Use somente as variáveis dos provedores do OpenMontage.",
        ));
    }
    Ok(())
}

fn normalized_package(name: &str) -> String {
    name.split('[')
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase()
        .replace(['_', '.'], "-")
}
fn package_versions(site: &Path) -> BTreeMap<String, String> {
    let Ok(entries) = fs::read_dir(site) else {
        return BTreeMap::new();
    };
    entries
        .flatten()
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|suffix| suffix == "dist-info")
        })
        .filter_map(|entry| fs::read_to_string(entry.path().join("METADATA")).ok())
        .filter_map(|metadata| {
            let name = metadata
                .lines()
                .find_map(|line| line.strip_prefix("Name: "))?;
            let version = metadata
                .lines()
                .find_map(|line| line.strip_prefix("Version: "))?;
            Some((normalized_package(name), version.into()))
        })
        .collect()
}
fn snapshot(home: &Path, secrets: &dyn Secrets) -> Result<Configuration, CoreError> {
    let settings = settings(home)?;
    let values = values(&settings, secrets)?;
    let generation = installed(home, ComponentId::Openmontage)
        .ok()
        .and_then(|record| record.path(home).ok());
    let completed: BTreeSet<String> = generation
        .as_ref()
        .and_then(|generation| fs::read(generation.join("optional-packages.json")).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let site_packages = generation.as_ref().map(|generation| {
        if cfg!(windows) {
            generation.join("venv/Lib/site-packages")
        } else {
            generation.join("venv/lib/python3.11/site-packages")
        }
    });
    let versions = site_packages
        .as_ref()
        .map(|site| package_versions(site))
        .unwrap_or_default();
    Ok(Configuration {
        allow_paid_tools: settings.allow_paid_tools,
        allow_model_downloads: settings.allow_model_downloads,
        credentials: CREDENTIAL_KEYS
            .split_whitespace()
            .map(|key| CredentialField {
                key: key.into(),
                label: key.replace('_', " "),
                configured: values.contains_key(key),
                secret: key.ends_with("KEY")
                    || key.ends_with("TOKEN")
                    || key.ends_with("SECRET")
                    || key.contains("_TOKEN_"),
            })
            .collect(),
        optional_packages: OPTIONAL
            .iter()
            .map(|(id, label, packages)| OptionalPackage {
                id: (*id).into(),
                label: (*label).into(),
                installed: completed.contains(*id)
                    && if *id == "animation" {
                        generation.as_ref().is_some_and(|generation| {
                            super::animation::runtime(generation)
                                .ok()
                                .flatten()
                                .is_some()
                        })
                    } else {
                        packages
                            .iter()
                            .all(|package| versions.contains_key(&normalized_package(package)))
                    },
            })
            .collect(),
    })
}

fn save(home: &Path, input: ConfigurationInput, secrets: &dyn Secrets) -> Result<(), CoreError> {
    validate_input(&input)?;
    let previous = settings(home)?;
    let mut values = values(&previous, secrets)?;
    for key in input.remove_credentials {
        values.remove(&key);
    }
    for (key, value) in input.credentials {
        if !value.trim().is_empty() {
            values.insert(key, value.trim().into());
        }
    }
    let reference = if values.is_empty() {
        None
    } else {
        let reference = format!(
            "jarvis-core-openmontage-{}",
            crate::library::new_id()
                .map_err(|_| error("Não foi possível salvar as credenciais."))?
        );
        secrets
            .store(
                &reference,
                &serde_json::to_string(&values).map_err(|_| error("Credenciais inválidas."))?,
            )
            .map_err(|_| {
                error("Não foi possível salvar as credenciais no armazenamento seguro.")
            })?;
        Some(reference)
    };
    let result = (|| {
        let path = path(home);
        let parent = path
            .parent()
            .ok_or_else(|| error("Configuração de vídeo inválida."))?;
        fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(
            &serde_json::to_vec(&Settings {
                allow_paid_tools: input.allow_paid_tools,
                allow_model_downloads: input.allow_model_downloads,
                credential_ref: reference.clone(),
            })
            .map_err(|_| error("Configuração inválida."))?,
        )?;
        file.as_file().sync_all()?;
        file.persist(path)
            .map_err(|_| error("Não foi possível salvar a configuração de vídeo."))?;
        Ok(())
    })();
    if result.is_err() {
        if let Some(reference) = reference {
            let _ = secrets.delete(&reference);
        }
    } else if let Some(reference) = previous.credential_ref {
        let _ = secrets.delete(&reference);
    }
    result
}

pub(super) fn apply_environment(
    home: &Path,
    environment: &mut BTreeMap<String, String>,
) -> Result<(), CoreError> {
    apply_with_secrets(home, environment, &Keychain)
}
fn apply_with_secrets(
    home: &Path,
    environment: &mut BTreeMap<String, String>,
    secrets: &dyn Secrets,
) -> Result<(), CoreError> {
    let settings = settings(home)?;
    for key in CREDENTIAL_KEYS.split_whitespace() {
        environment.insert(key.into(), String::new());
    }
    environment.extend(values(&settings, secrets)?);
    environment.insert(
        "JARVIS_OPENMONTAGE_ALLOW_PAID".into(),
        if settings.allow_paid_tools { "1" } else { "0" }.into(),
    );
    environment.insert(
        "JARVIS_OPENMONTAGE_ALLOW_MODEL_DOWNLOADS".into(),
        if settings.allow_model_downloads {
            "1"
        } else {
            "0"
        }
        .into(),
    );
    for key in ["HF_HUB_OFFLINE", "TRANSFORMERS_OFFLINE"] {
        environment.insert(
            key.into(),
            if settings.allow_model_downloads {
                "0"
            } else {
                "1"
            }
            .into(),
        );
    }
    Ok(())
}

#[tauri::command]
pub async fn get_openmontage_configuration(
    app: tauri::AppHandle,
) -> Result<Configuration, CoreError> {
    let home = app
        .path()
        .home_dir()
        .map_err(|_| error("Pasta pessoal indisponível."))?;
    tokio::task::spawn_blocking(move || snapshot(&home, &Keychain))
        .await
        .map_err(|_| error("Configuração de vídeo indisponível."))?
}
#[tauri::command]
pub async fn save_openmontage_configuration(
    app: tauri::AppHandle,
    configuration: ConfigurationInput,
) -> Result<Configuration, CoreError> {
    let home = app
        .path()
        .home_dir()
        .map_err(|_| error("Pasta pessoal indisponível."))?;
    tokio::task::spawn_blocking(move || {
        save(&home, configuration, &Keychain)?;
        snapshot(&home, &Keychain)
    })
    .await
    .map_err(|_| error("Configuração de vídeo indisponível."))?
}
#[tauri::command]
pub async fn install_openmontage_optional_package(
    app: tauri::AppHandle,
    core: tauri::State<'_, super::super::CoreState>,
    id: String,
) -> Result<Configuration, CoreError> {
    let _activity = crate::updater::begin_activity(&app).map_err(|message| error(&message))?;
    let _installation = core
        .install_lock
        .try_lock()
        .map_err(|_| error("Aguarde a instalação atual do Core terminar."))?;
    let home = app
        .path()
        .home_dir()
        .map_err(|_| error("Pasta pessoal indisponível."))?;
    let (_, _, packages) = OPTIONAL
        .iter()
        .find(|(candidate, _, _)| *candidate == id)
        .ok_or_else(|| error("Pacote opcional desconhecido."))?;
    let generation = installed(&home, ComponentId::Openmontage)?.path(&home)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(super::super::root(&home).join("install.lock"))?;
    fs2::FileExt::try_lock_exclusive(&lock)
        .map_err(|_| error("Outro Jarvis está instalando o Core."))?;
    let operation = core.begin_installation(ComponentId::Openmontage)?;
    let mut signal = operation.signal.clone();
    core.stage(
        &app,
        &home,
        ComponentId::Openmontage,
        "Preparando dependências opcionais",
    );
    let result = tokio::select! {
        biased;
        _=super::super::context::cancelled(&mut signal)=>Err(super::super::cancelled_error()),
        result=install_optional(&generation,&home,&id,packages)=>result,
    };
    if let Ok(mut data) = core.data.lock() {
        data.stages.remove(&ComponentId::Openmontage);
    }
    core.emit(&app, &home);
    result?;
    tokio::task::spawn_blocking(move || snapshot(&home, &Keychain))
        .await
        .map_err(|_| error("Configuração de vídeo indisponível."))?
}

async fn install_optional(
    generation: &Path,
    home: &Path,
    id: &str,
    packages: &[&str],
) -> Result<(), CoreError> {
    if id == "animation" {
        super::animation::install(generation, home).await?;
    } else {
        let mut command = pip(generation, home);
        command.args(packages);
        super::install::command_unbounded(command).await?;
        let mut command = tokio::process::Command::new(python_path(generation));
        command
            .args(["-m", "pip", "--isolated", "freeze"])
            .env_remove("PYTHONHOME")
            .env_remove("PYTHONPATH");
        fs::write(
            generation.join("python-requirements.lock.txt"),
            super::install::command(command, 30).await?,
        )?;
    }
    let mut completed: BTreeSet<String> = fs::read(generation.join("optional-packages.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    completed.insert(id.into());
    let mut receipt = tempfile::NamedTempFile::new_in(generation)?;
    receipt.write_all(
        &serde_json::to_vec(&completed).map_err(|_| error("Registro opcional inválido."))?,
    )?;
    receipt.as_file().sync_all()?;
    receipt
        .persist(generation.join("optional-packages.json"))
        .map_err(|_| error("Não foi possível salvar a instalação opcional."))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    #[derive(Default)]
    struct Memory(Mutex<BTreeMap<String, String>>);
    impl Secrets for Memory {
        fn load(&self, key: &str) -> Result<String, crate::mcp::McpError> {
            self.0
                .lock()
                .unwrap()
                .get(key)
                .cloned()
                .ok_or_else(|| crate::mcp::error("not found"))
        }
        fn store(&self, key: &str, value: &str) -> Result<(), crate::mcp::McpError> {
            self.0.lock().unwrap().insert(key.into(), value.into());
            Ok(())
        }
        fn delete(&self, key: &str) -> Result<(), crate::mcp::McpError> {
            self.0.lock().unwrap().remove(key);
            Ok(())
        }
    }
    fn input() -> ConfigurationInput {
        ConfigurationInput {
            allow_paid_tools: false,
            allow_model_downloads: false,
            credentials: BTreeMap::new(),
            remove_credentials: Vec::new(),
        }
    }
    #[test]
    fn credentials_stay_in_vault_and_are_redacted_in_ui() {
        let home = tempfile::tempdir().unwrap();
        let secrets = Memory::default();
        let mut config = input();
        config
            .credentials
            .insert("ELEVENLABS_API_KEY".into(), "synthetic-test-secret".into());
        save(home.path(), config, &secrets).unwrap();
        assert!(!fs::read_to_string(path(home.path()))
            .unwrap()
            .contains("synthetic-test-secret"));
        let state = snapshot(home.path(), &secrets).unwrap();
        assert!(
            state
                .credentials
                .iter()
                .find(|field| field.key == "ELEVENLABS_API_KEY")
                .unwrap()
                .configured
        );
        assert!(!serde_json::to_string(&state)
            .unwrap()
            .contains("synthetic-test-secret"));
        let mut env = BTreeMap::from([("OPENAI_API_KEY".into(), "inherited-key".into())]);
        apply_with_secrets(home.path(), &mut env, &secrets).unwrap();
        assert_eq!(env["OPENAI_API_KEY"], "");
        assert_eq!(env["ELEVENLABS_API_KEY"], "synthetic-test-secret");
        assert_eq!(env["HF_HUB_OFFLINE"], "1");
        let mut config = input();
        config.remove_credentials.push("ELEVENLABS_API_KEY".into());
        save(home.path(), config, &secrets).unwrap();
        assert!(secrets.0.lock().unwrap().is_empty());
    }
    #[test]
    fn unsafe_environment_and_unbounded_secret_payloads_are_rejected() {
        let mut config = input();
        config.credentials.insert("PATH".into(), "/outside".into());
        assert!(validate_input(&config).is_err());
        config.credentials.clear();
        config
            .credentials
            .insert("FAL_KEY".into(), "x".repeat(16_385));
        assert!(validate_input(&config).is_err());
    }
    #[test]
    fn partial_optional_installation_is_not_presented_as_ready() {
        let home = tempfile::tempdir().unwrap();
        let directory = "openmontage/test";
        let generation = crate::core::root(home.path()).join(directory);
        crate::core::openmontage::tests::fixture(&generation);
        let manifest = crate::core::Manifest {
            installations: BTreeMap::from([(
                ComponentId::Openmontage,
                crate::core::Installation {
                    version: crate::core::openmontage::VERSION.into(),
                    directory: directory.into(),
                    files: crate::core::openmontage::required_files(&generation).unwrap(),
                },
            )]),
        };
        crate::core::save_manifest(home.path(), &manifest).unwrap();
        let site = generation.join(if cfg!(windows) {
            "venv/Lib/site-packages"
        } else {
            "venv/lib/python3.11/site-packages"
        });
        let installed = || {
            snapshot(home.path(), &Memory::default())
                .unwrap()
                .optional_packages
                .into_iter()
                .find(|group| group.id == "analysis")
                .unwrap()
                .installed
        };
        let packages = OPTIONAL
            .iter()
            .find(|(id, _, _)| *id == "analysis")
            .unwrap()
            .2;
        for package in packages {
            let name = normalized_package(package);
            let metadata = site.join(format!("{name}-1.0.dist-info/METADATA"));
            fs::create_dir_all(metadata.parent().unwrap()).unwrap();
            fs::write(metadata, format!("Name: {name}\nVersion: 1.0\n")).unwrap();
        }
        assert!(
            !installed(),
            "Packages alone do not prove the optional install completed"
        );
        fs::write(generation.join("optional-packages.json"), r#"["analysis"]"#).unwrap();
        assert!(installed());
        let metadata = site.join("manim-0.19.0.dist-info/METADATA");
        fs::create_dir_all(metadata.parent().unwrap()).unwrap();
        fs::write(metadata, "Name: manim\nVersion: 0.19.0\n").unwrap();
        fs::write(
            generation.join("optional-packages.json"),
            r#"["analysis","animation"]"#,
        )
        .unwrap();
        let animation = snapshot(home.path(), &Memory::default())
            .unwrap()
            .optional_packages
            .into_iter()
            .find(|group| group.id == "animation")
            .unwrap();
        assert!(
            !animation.installed,
            "A legacy pip-only Manim install has no verified private runtime"
        );
        assert_eq!(animation.label, "Animações Manim (Cairo e Pango incluídos)");
        assert!(installed(), "Other completed package groups remain ready");
        fs::remove_file(site.join("faster-whisper-1.0.dist-info/METADATA")).unwrap();
        assert!(
            !installed(),
            "A partial package group must remain unavailable"
        );
    }
}
