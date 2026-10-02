//! Pinned, verified downloads. No model fetch is hidden inside microphone activation.
use super::{Config, VoiceState};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};
use tauri::{Emitter, Manager};
use tokio::io::AsyncWriteExt;

async fn cancelled(state: &VoiceState) {
    while !state.cancel_download.load(Ordering::Acquire) {
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
}

const WHISPER_REV: &str = "5359861c739e955e79d9a303bcbc70fb988958b1";
const VAD_REV: &str = "9ffd54a1e1ee413ddf265af9913beaf518d1639b";
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Model {
    pub id: &'static str,
    pub name: &'static str,
    pub bytes: u64,
    #[serde(skip)]
    pub file: &'static str,
    #[serde(skip)]
    pub sha256: &'static str,
    pub installed: bool,
}
pub(super) const MODELS: [Model; 2] = [
    Model {
        id: "small",
        name: "Small · melhor precisão",
        file: "ggml-small-q5_1.bin",
        bytes: 190085487,
        sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
        installed: false,
    },
    Model {
        id: "tiny",
        name: "Tiny · mais rápido",
        file: "ggml-tiny-q5_1.bin",
        bytes: 32152673,
        sha256: "818710568da3ca15689e31a743197b520007872ff9576237bda97bd1b469c3d7",
        installed: false,
    },
];
const VAD: Model = Model {
    id: "vad",
    name: "Silero VAD",
    file: "ggml-silero-v6.2.0.bin",
    bytes: 885098,
    sha256: "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987",
    installed: false,
};

pub(super) fn root(home: &Path) -> PathBuf {
    crate::data_dir::root(home).join("models/voice")
}
pub(super) fn selected(config: &Config) -> &'static Model {
    MODELS
        .iter()
        .find(|model| model.id == config.model)
        .unwrap_or(&MODELS[0])
}
fn valid(root: &Path, model: &Model) -> bool {
    fs::metadata(root.join(model.file)).is_ok_and(|m| m.is_file() && m.len() == model.bytes)
        && fs::read_to_string(root.join(format!("{}.sha256", model.file)))
            .is_ok_and(|hash| hash == model.sha256)
}
pub(super) fn list(home: &Path) -> Vec<Model> {
    let root = root(home);
    MODELS
        .iter()
        .cloned()
        .map(|mut model| {
            model.installed = valid(&root, &model) && valid(&root, &VAD);
            model
        })
        .collect()
}
pub(super) fn paths(home: &Path, config: &Config) -> Result<(PathBuf, PathBuf), String> {
    let root = root(home);
    let model = selected(config);
    if !valid(&root, model) || !valid(&root, &VAD) {
        return Err("Prepare o modelo de transcrição nas configurações do Jarvis Voice.".into());
    }
    Ok((root.join(model.file), root.join(VAD.file)))
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Download {
    pub model: String,
    pub received: u64,
    pub total: u64,
}

#[tauri::command]
pub(crate) async fn install_voice_model(
    app: tauri::AppHandle,
    model: String,
) -> Result<(), String> {
    let state = app.state::<VoiceState>();
    let model = MODELS
        .iter()
        .find(|item| item.id == model)
        .ok_or("Modelo de transcrição inválido.")?;
    if state.downloading.swap(true, Ordering::AcqRel) {
        return Err("Já existe um download de voz em andamento.".into());
    }
    state.cancel_download.store(false, Ordering::Release);
    let result = async {
        let home = app
            .path()
            .home_dir()
            .map_err(|_| "Diretório de dados indisponível.")?;
        let root = root(&home);
        fs::create_dir_all(&root).map_err(|_| "Não foi possível preparar os modelos de voz.")?;
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .read_timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|_| "Cliente de download indisponível.")?;
        let total = model.bytes + VAD.bytes;
        let mut received = 0;
        *state.download.lock().map_err(|_| "Estado da voz indisponível.")? = Some(Download { model: model.id.into(), received, total });
        let _ = app.emit("voice:changed", ());
        let mut last_progress = std::time::Instant::now();
        for asset in [model, &VAD] {
            if valid(&root, asset) {
                received += asset.bytes;
                continue;
            }
            let (repo, revision) = if asset.id == "vad" {
                ("ggml-org/whisper-vad", VAD_REV)
            } else {
                ("ggerganov/whisper.cpp", WHISPER_REV)
            };
            let url = format!(
                "https://huggingface.co/{repo}/resolve/{revision}/{}",
                asset.file
            );
            let response = tokio::select! {
                response = client.get(url).send() => response.map_err(|_| "A conexão caiu durante o download. Tente novamente.")?,
                _ = cancelled(&state) => return Err("Download cancelado.".into()),
            };
            let mut response = response
                .error_for_status()
                .map_err(|_| "O servidor não disponibilizou o modelo de voz. Tente novamente.")?;
            let staged = tempfile::NamedTempFile::new_in(&root)
                .map_err(|_| "Não foi possível gravar o modelo de voz.")?;
            let mut file = tokio::fs::File::from_std(
                staged
                    .reopen()
                    .map_err(|_| "Arquivo temporário indisponível.")?,
            );
            let mut hash = Sha256::new();
            let mut length = 0;
            loop {
                let chunk = tokio::select! {
                    chunk = response.chunk() => chunk.map_err(|_| "O download foi interrompido. Tente novamente.")?,
                    _ = cancelled(&state) => return Err("Download cancelado.".into()),
                };
                let Some(chunk) = chunk else { break; };
                if state.cancel_download.load(Ordering::Acquire) {
                    return Err("Download cancelado.".to_owned());
                }
                length += chunk.len() as u64;
                if length > asset.bytes {
                    return Err("O tamanho do modelo recebido é inválido.".into());
                }
                hash.update(&chunk);
                file.write_all(&chunk)
                    .await
                    .map_err(|_| "Não foi possível salvar o modelo de voz.")?;
                received += chunk.len() as u64;
                let progress = Download {
                    model: model.id.into(),
                    received,
                    total,
                };
                *state
                    .download
                    .lock()
                    .map_err(|_| "Estado da voz indisponível.")? = Some(progress.clone());
                if last_progress.elapsed() >= std::time::Duration::from_millis(180) {
                    let _ = app.emit("voice:download", progress); last_progress = std::time::Instant::now();
                }
            }
            if length != asset.bytes || format!("{:x}", hash.finalize()) != asset.sha256 {
                return Err(
                    "A integridade do modelo de voz não foi confirmada. Tente novamente.".into(),
                );
            }
            file.sync_all()
                .await
                .map_err(|_| "Não foi possível concluir o download.")?;
            drop(file);
            if state.cancel_download.load(Ordering::Acquire) {
                return Err("Download cancelado.".into());
            }
            staged
                .persist(root.join(asset.file))
                .map_err(|_| "Não foi possível instalar o modelo de voz.")?;
            fs::write(root.join(format!("{}.sha256", asset.file)), asset.sha256)
                .map_err(|_| "Não foi possível registrar o modelo de voz.")?;
        }
        Ok(())
    }
    .await;
    state.downloading.store(false, Ordering::Release);
    if let Ok(mut download) = state.download.lock() {
        *download = None;
    }
    let _ = app.emit("voice:changed", ());
    result
}

#[tauri::command]
pub(crate) fn cancel_voice_download(state: tauri::State<'_, VoiceState>) {
    state.cancel_download.store(true, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_incomplete_or_unverified_model_cannot_open_the_microphone() {
        let home = tempfile::tempdir().unwrap();
        assert!(paths(home.path(), &Config::default()).is_err());
        assert!(list(home.path()).iter().all(|model| !model.installed));
        assert!(MODELS
            .iter()
            .all(|model| model.sha256.len() == 64 && model.bytes > 0));
        assert_eq!(selected(&Config::default()).id, "small");
    }
}
