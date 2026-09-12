//! Portable, provider-free settings backups.

use crate::{
    agent::{self, workflow},
    mcp::{self, McpState},
    openai_codex::OpenAiCodexState,
    persistence::AppState,
    skills, system,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Cursor, Read, Write},
    path::{Component, Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager};
use zip::write::SimpleFileOptions;

const FORMAT: &str = "jarvis-settings-backup";
const FORMAT_VERSION: u16 = 1;
const MANIFEST_NAME: &str = "manifest.json";
const SETTINGS_NAME: &str = "settings.json";
const MAX_ARCHIVE_BYTES: u64 = 96 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 64 * 1024 * 1024;
const MAX_SETTINGS_BYTES: u64 = 8 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_ENTRIES: usize = 4096;
const MAX_DEPTH: usize = 18;
static LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Serialize)]
pub struct BackupError {
    code: &'static str,
    message: String,
}

fn error(message: impl Into<String>) -> BackupError {
    BackupError {
        code: "settings_backup_error",
        message: message.into(),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelTargetKind {
    BuiltinAgent,
    CustomAgent,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelTarget {
    id: String,
    kind: ModelTargetKind,
    label: String,
    details: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupSummary {
    custom_agents: usize,
    custom_flows: usize,
    skills: usize,
    mcps: usize,
    model_targets: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    format: String,
    version: u16,
    created_at: u64,
    app_version: String,
    settings_sha256: String,
    summary: BackupSummary,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SettingsPayload {
    system: system::Preferences,
    catalog: workflow::catalog::Catalog,
    model_targets: Vec<ModelTarget>,
    skills: skills::PortableConfig,
    mcps: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupExportResult {
    path: String,
    bytes: u64,
    summary: BackupSummary,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPreview {
    fingerprint: String,
    created_at: u64,
    app_version: String,
    archive_bytes: u64,
    summary: BackupSummary,
    model_targets: Vec<ModelTarget>,
    warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelMapping {
    target_id: String,
    choice: workflow::settings::ModelChoice,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupImportResult {
    summary: BackupSummary,
    mapped_models: usize,
}

#[derive(Clone, Debug)]
struct SkillFile {
    path: PathBuf,
    bytes: Vec<u8>,
    mode: u32,
}

#[derive(Debug)]
struct LoadedBackup {
    manifest: Manifest,
    payload: SettingsPayload,
    skill_files: Vec<SkillFile>,
    archive_bytes: u64,
    fingerprint: String,
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn builtin_target(key: &str) -> Option<ModelTarget> {
    let (flow, role) = key.split_once('/')?;
    let (flow_label, roles): (&str, &[(&str, &str)]) = match flow {
        "standard" => ("Padrão", &[("builder", "Construtor")]),
        "designer" => ("Designer", &[("designer", "Designer")]),
        "planned" => (
            "Planejado",
            &[
                ("planner", "Planejador"),
                ("builder", "Construtor"),
                ("designer", "Designer"),
            ],
        ),
        "complete" => (
            "Completo",
            &[
                ("planner", "Planejador"),
                ("investigator", "Investigador"),
                ("writer", "Redator"),
                ("orchestrator", "Orquestrador"),
                ("designer", "Designer"),
                ("builder", "Construtor"),
                ("reviewer", "Revisor"),
            ],
        ),
        "publication" => ("GitHub", &[("github", "GitHub")]),
        _ => return None,
    };
    let role_label = roles
        .iter()
        .find_map(|(value, label)| (*value == role).then_some(*label))?;
    Some(ModelTarget {
        id: format!("builtin:{key}"),
        kind: ModelTargetKind::BuiltinAgent,
        label: role_label.into(),
        details: vec![format!("Fluxo {flow_label}"), "Agente Jarvis".into()],
    })
}

fn model_targets(
    native: &workflow::settings::ModelSettings,
    catalog: &workflow::catalog::Catalog,
) -> Result<Vec<ModelTarget>, BackupError> {
    let mut targets = Vec::with_capacity(native.len() + catalog.agents.len());
    for key in native.keys() {
        targets.push(
            builtin_target(key)
                .ok_or_else(|| error("A configuração dos agentes nativos é inválida."))?,
        );
    }
    targets.extend(catalog.agents.iter().map(|agent| ModelTarget {
        id: format!("custom:{}", agent.id),
        kind: ModelTargetKind::CustomAgent,
        label: agent.name.clone(),
        details: vec!["Agente customizado".into()],
    }));
    targets.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(targets)
}

fn clean_catalog(mut catalog: workflow::catalog::Catalog) -> workflow::catalog::Catalog {
    catalog.revision = 0;
    for agent in &mut catalog.agents {
        agent.model = None;
    }
    catalog
}

fn validate_payload(payload: &SettingsPayload) -> Result<(), BackupError> {
    payload
        .system
        .validate()
        .map_err(|message| error(format!("Preferências inválidas no backup: {message}")))?;
    if payload.catalog.revision != 0
        || payload
            .catalog
            .agents
            .iter()
            .any(|agent| agent.model.is_some())
    {
        return Err(error(
            "O backup contém vínculos de provedores. Por segurança, ele não pode ser importado.",
        ));
    }
    payload
        .catalog
        .validate()
        .map_err(|_| error("Os agentes ou fluxos do backup são inválidos."))?;
    payload
        .skills
        .validate()
        .map_err(|_| error("A configuração de skills do backup é inválida."))?;
    if payload.mcps.len() > 32 {
        return Err(error("O backup contém mais de 32 MCPs."));
    }
    let mut mcp_names = BTreeSet::new();
    for raw in &payload.mcps {
        let (name, _) = mcp::config::parse(raw)
            .map_err(|_| error("O backup contém uma configuração de MCP inválida."))?;
        if !mcp_names.insert(name) {
            return Err(error("O backup contém MCPs com nomes repetidos."));
        }
    }

    let mut ids = BTreeSet::new();
    let custom_ids: BTreeSet<_> = payload
        .catalog
        .agents
        .iter()
        .map(|agent| format!("custom:{}", agent.id))
        .collect();
    let mut target_custom_ids = BTreeSet::new();
    for target in &payload.model_targets {
        if !ids.insert(target.id.as_str())
            || target.label.trim().is_empty()
            || target.label.len() > 100
            || target.details.len() > 4
            || target
                .details
                .iter()
                .any(|detail| detail.is_empty() || detail.len() > 200)
        {
            return Err(error("Os destinos de modelo do backup são inválidos."));
        }
        match target.kind {
            ModelTargetKind::BuiltinAgent => {
                let key = target.id.strip_prefix("builtin:").ok_or_else(|| {
                    error("Um agente nativo do backup possui um identificador inválido.")
                })?;
                if builtin_target(key).as_ref() != Some(target) {
                    return Err(error(
                        "Um agente nativo do backup não é reconhecido por esta versão ou possui dados inconsistentes.",
                    ));
                }
            }
            ModelTargetKind::CustomAgent => {
                let agent = target
                    .id
                    .strip_prefix("custom:")
                    .and_then(|id| payload.catalog.agents.iter().find(|agent| agent.id == id));
                if !custom_ids.contains(&target.id)
                    || agent.is_none_or(|agent| {
                        target.label != agent.name
                            || target.details != ["Agente customizado".to_string()]
                    })
                {
                    return Err(error(
                        "Um destino de modelo não corresponde aos agentes do backup.",
                    ));
                }
                target_custom_ids.insert(target.id.clone());
            }
        }
    }
    if target_custom_ids != custom_ids {
        return Err(error(
            "O backup não contém todos os destinos dos agentes customizados.",
        ));
    }
    Ok(())
}

fn collect_skill_files(root: &Path) -> Result<Vec<SkillFile>, BackupError> {
    fn walk(
        root: &Path,
        directory: &Path,
        depth: usize,
        total: &mut u64,
        files: &mut Vec<SkillFile>,
    ) -> Result<(), BackupError> {
        if depth > MAX_DEPTH {
            return Err(error("As skills possuem pastas demais para o backup."));
        }
        let mut entries: Vec<_> = fs::read_dir(directory)
            .map_err(|_| error("Não foi possível ler as skills instaladas."))?
            .collect::<Result<_, _>>()
            .map_err(|_| error("Não foi possível ler as skills instaladas."))?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            if files.len() >= MAX_ENTRIES {
                return Err(error("As skills excedem o limite de arquivos do backup."));
            }
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|_| error("Não foi possível verificar uma skill instalada."))?;
            if metadata.file_type().is_symlink() {
                return Err(error(
                    "Uma skill instalada contém um atalho. Remova o atalho antes de criar o backup.",
                ));
            }
            if metadata.is_dir() {
                walk(root, &path, depth + 1, total, files)?;
                continue;
            }
            if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
                return Err(error(
                    "Uma skill contém um arquivo inválido ou muito grande.",
                ));
            }
            *total = total
                .checked_add(metadata.len())
                .filter(|value| *value <= MAX_EXPANDED_BYTES)
                .ok_or_else(|| error("As skills excedem 64 MB no backup."))?;
            let relative = path
                .strip_prefix(root)
                .map_err(|_| error("A skill possui um caminho inválido."))?
                .to_path_buf();
            validate_skill_path(&relative)?;
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode() & 0o777
            };
            #[cfg(not(unix))]
            let mode = 0o644;
            files.push(SkillFile {
                path: relative,
                bytes: fs::read(&path)
                    .map_err(|_| error("Não foi possível ler uma skill instalada."))?,
                mode,
            });
        }
        Ok(())
    }

    if !root.exists() {
        return Ok(Vec::new());
    }
    let metadata = fs::symlink_metadata(root)
        .map_err(|_| error("Não foi possível verificar a pasta de skills."))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(error("A pasta de skills do Jarvis é inválida."));
    }
    let mut files = Vec::new();
    let mut total = 0;
    walk(root, root, 0, &mut total, &mut files)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    let mut portable = BTreeSet::new();
    if files.iter().any(|file| {
        !portable.insert(
            file.path
                .to_string_lossy()
                .to_lowercase()
                .replace('\\', "/"),
        )
    }) {
        return Err(error(
            "Duas skills possuem caminhos que entram em conflito em outros sistemas.",
        ));
    }
    Ok(files)
}

fn skill_count(files: &[SkillFile]) -> usize {
    files
        .iter()
        .filter_map(|file| file.path.components().next())
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_os_string()),
            _ => None,
        })
        .collect::<BTreeSet<_>>()
        .len()
}

fn summary(payload: &SettingsPayload, files: &[SkillFile]) -> BackupSummary {
    BackupSummary {
        custom_agents: payload.catalog.agents.len(),
        custom_flows: payload.catalog.flows.len(),
        skills: skill_count(files),
        mcps: payload.mcps.len(),
        model_targets: payload.model_targets.len(),
    }
}

fn zip_options(mode: u32) -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(mode)
}

fn write_archive(
    path: &Path,
    payload: &SettingsPayload,
    skill_files: &[SkillFile],
) -> Result<u64, BackupError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| error("Escolha uma pasta válida para o backup."))?;
    fs::create_dir_all(parent)
        .map_err(|_| error("Não foi possível preparar a pasta do backup."))?;
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(error("O destino do backup não pode ser um atalho."));
    }
    let settings = serde_json::to_vec_pretty(payload)
        .map_err(|_| error("Não foi possível preparar as configurações."))?;
    let archive_summary = summary(payload, skill_files);
    let manifest = Manifest {
        format: FORMAT.into(),
        version: FORMAT_VERSION,
        created_at: unix_timestamp(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        settings_sha256: digest(&settings),
        summary: archive_summary,
    };
    let manifest = serde_json::to_vec_pretty(&manifest)
        .map_err(|_| error("Não foi possível preparar o manifesto do backup."))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| error("Não foi possível criar o arquivo do backup."))?;
    {
        let mut archive = zip::ZipWriter::new(temporary.as_file_mut());
        archive
            .start_file(MANIFEST_NAME, zip_options(0o600))
            .map_err(|_| error("Não foi possível iniciar o backup."))?;
        archive
            .write_all(&manifest)
            .map_err(|_| error("Não foi possível gravar o manifesto do backup."))?;
        archive
            .start_file(SETTINGS_NAME, zip_options(0o600))
            .map_err(|_| error("Não foi possível gravar as configurações."))?;
        archive
            .write_all(&settings)
            .map_err(|_| error("Não foi possível gravar as configurações."))?;
        for file in skill_files {
            let relative = file
                .path
                .to_str()
                .ok_or_else(|| error("Uma skill possui um nome incompatível."))?
                .replace('\\', "/");
            archive
                .start_file(format!("skills/{relative}"), zip_options(file.mode))
                .map_err(|_| error("Não foi possível adicionar uma skill ao backup."))?;
            archive
                .write_all(&file.bytes)
                .map_err(|_| error("Não foi possível adicionar uma skill ao backup."))?;
        }
        archive
            .finish()
            .map_err(|_| error("Não foi possível finalizar o arquivo de backup."))?;
    }
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| error("Não foi possível finalizar o arquivo de backup."))?;
    let bytes = temporary
        .as_file()
        .metadata()
        .map_err(|_| error("Não foi possível verificar o arquivo de backup."))?
        .len();
    temporary
        .persist(path)
        .map_err(|_| error("Não foi possível salvar o backup no local escolhido."))?;
    #[cfg(unix)]
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| error("Não foi possível concluir o backup com segurança."))?;
    Ok(bytes)
}

fn safe_archive_path(name: &str) -> Result<PathBuf, BackupError> {
    if name.is_empty() || name.contains(['\\', '\0']) {
        return Err(error("O backup contém um caminho inseguro."));
    }
    let path = Path::new(name);
    if path.is_absolute()
        || path.components().count() > MAX_DEPTH
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(error("O backup contém um caminho inseguro."));
    }
    Ok(path.to_path_buf())
}

fn validate_skill_path(path: &Path) -> Result<(), BackupError> {
    let components: Vec<_> = path.components().collect();
    if components.len() < 2 || components.len() > MAX_DEPTH {
        return Err(error("Uma skill do backup possui um caminho inválido."));
    }
    for component in components {
        let Component::Normal(segment) = component else {
            return Err(error("Uma skill do backup possui um caminho inseguro."));
        };
        let segment = segment
            .to_str()
            .ok_or_else(|| error("Uma skill possui um nome incompatível com o backup."))?;
        if segment.is_empty()
            || segment.len() > 255
            || segment.ends_with([' ', '.'])
            || segment
                .chars()
                .any(|character| character.is_control() || "<>:\"/\\|?*".contains(character))
        {
            return Err(error(
                "Uma skill possui um nome de arquivo incompatível com macOS e Windows.",
            ));
        }
        let stem = segment
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(stem.as_str(), "con" | "prn" | "aux" | "nul")
            || stem.strip_prefix("com").is_some_and(|number| {
                matches!(number, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
            || stem.strip_prefix("lpt").is_some_and(|number| {
                matches!(number, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
        {
            return Err(error(
                "Uma skill usa um nome reservado e não pode ser restaurada em todos os sistemas.",
            ));
        }
    }
    Ok(())
}

fn read_archive(path: &Path) -> Result<LoadedBackup, BackupError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| error("Não foi possível abrir o arquivo de backup."))?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_ARCHIVE_BYTES
    {
        return Err(error("Escolha um backup ZIP válido de até 96 MB."));
    }
    let bytes = fs::read(path).map_err(|_| error("Não foi possível ler o arquivo de backup."))?;
    let fingerprint = digest(&bytes);
    let mut archive = zip::ZipArchive::new(Cursor::new(&bytes))
        .map_err(|_| error("O arquivo escolhido não é um backup ZIP válido."))?;
    if archive.is_empty() || archive.len() > MAX_ENTRIES + 2 {
        return Err(error(
            "O backup possui uma quantidade inválida de arquivos.",
        ));
    }
    let mut seen = BTreeSet::new();
    let mut manifest = None;
    let mut settings = None;
    let mut skill_files = Vec::new();
    let mut expanded = 0_u64;
    for index in 0..archive.len() {
        let mut item = archive
            .by_index(index)
            .map_err(|_| error("Não foi possível ler uma entrada do backup."))?;
        let name = item.name().to_owned();
        let path = safe_archive_path(name.trim_end_matches('/'))?;
        let portable_name = path.to_string_lossy().to_lowercase().replace('\\', "/");
        if !seen.insert(portable_name) || item.is_symlink() || (!item.is_file() && !item.is_dir()) {
            return Err(error(
                "O backup contém entradas repetidas ou não suportadas.",
            ));
        }
        if item.is_dir() {
            if path != Path::new("skills") && !path.starts_with("skills") {
                return Err(error("O backup contém uma pasta desconhecida."));
            }
            continue;
        }
        let size = item.size();
        let limit = if path == Path::new(MANIFEST_NAME) {
            64 * 1024
        } else if path == Path::new(SETTINGS_NAME) {
            MAX_SETTINGS_BYTES
        } else {
            MAX_FILE_BYTES
        };
        if size > limit {
            return Err(error("O backup contém um arquivo maior que o permitido."));
        }
        expanded = expanded
            .checked_add(size)
            .filter(|value| *value <= MAX_EXPANDED_BYTES)
            .ok_or_else(|| error("O conteúdo expandido do backup excede 64 MB."))?;
        let mut content = Vec::with_capacity(size as usize);
        item.read_to_end(&mut content)
            .map_err(|_| error("Não foi possível validar o conteúdo do backup."))?;
        if content.len() as u64 != size {
            return Err(error("Um arquivo do backup está incompleto."));
        }
        if path == Path::new(MANIFEST_NAME) {
            manifest = Some(content);
        } else if path == Path::new(SETTINGS_NAME) {
            settings = Some(content);
        } else {
            let relative = path
                .strip_prefix("skills")
                .map_err(|_| error("O backup contém um arquivo desconhecido."))?;
            validate_skill_path(relative)?;
            let mode = item.unix_mode().unwrap_or(0o644) & 0o777;
            skill_files.push(SkillFile {
                path: relative.to_path_buf(),
                bytes: content,
                mode: if mode == 0 { 0o644 } else { mode },
            });
        }
    }
    let manifest: Manifest =
        serde_json::from_slice(&manifest.ok_or_else(|| error("O backup não contém o manifesto."))?)
            .map_err(|_| error("O manifesto do backup é inválido."))?;
    if manifest.format != FORMAT {
        return Err(error("O arquivo escolhido não foi criado pelo Jarvis."));
    }
    if manifest.version > FORMAT_VERSION {
        return Err(error(
            "Este backup foi criado por uma versão mais nova do Jarvis. Atualize o aplicativo antes de restaurá-lo.",
        ));
    }
    if manifest.version != FORMAT_VERSION {
        return Err(error("Esta versão do formato de backup não é suportada."));
    }
    let settings = settings.ok_or_else(|| error("O backup não contém as configurações."))?;
    if manifest.settings_sha256 != digest(&settings) {
        return Err(error(
            "A verificação de integridade das configurações falhou.",
        ));
    }
    let payload: SettingsPayload = serde_json::from_slice(&settings)
        .map_err(|_| error("As configurações do backup são inválidas."))?;
    validate_payload(&payload)?;
    if manifest.summary != summary(&payload, &skill_files) {
        return Err(error(
            "O resumo do backup não corresponde ao conteúdo do arquivo.",
        ));
    }
    Ok(LoadedBackup {
        manifest,
        payload,
        skill_files,
        archive_bytes: metadata.len(),
        fingerprint,
    })
}

fn preview(loaded: &LoadedBackup) -> BackupPreview {
    let mut warnings = vec![
        "Provedores, contas, credenciais de IA e modelos não fazem parte do backup.".into(),
        "A restauração substitui as preferências, os agentes, os fluxos, as skills e os MCPs atuais.".into(),
        "Projetos, conversas e pacotes instalados do Core permanecem nesta instalação.".into(),
    ];
    if !loaded.payload.mcps.is_empty() {
        warnings.push(
            "Configurações de MCP podem conter chaves de acesso. Guarde este ZIP em local seguro."
                .into(),
        );
    }
    if loaded.payload.skills.include_agents {
        warnings.push(
            "Atalhos e pastas externas de .agents/skills não são copiados; a preferência de leitura será preservada."
                .into(),
        );
    }
    BackupPreview {
        fingerprint: loaded.fingerprint.clone(),
        created_at: loaded.manifest.created_at,
        app_version: loaded.manifest.app_version.clone(),
        archive_bytes: loaded.archive_bytes,
        summary: loaded.manifest.summary.clone(),
        model_targets: loaded.payload.model_targets.clone(),
        warnings,
    }
}

fn write_staged_file(path: &Path, bytes: &[u8]) -> Result<(), BackupError> {
    let parent = path
        .parent()
        .ok_or_else(|| error("O caminho temporário da restauração é inválido."))?;
    fs::create_dir_all(parent).map_err(|_| error("Não foi possível preparar a restauração."))?;
    let mut file =
        fs::File::create(path).map_err(|_| error("Não foi possível preparar a restauração."))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| error("Não foi possível preparar a restauração."))
}

fn remove_path(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(path)
        }
        Ok(_) => fs::remove_file(path),
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(cause) => Err(cause),
    }
}

#[derive(Debug)]
struct SwappedTarget {
    name: &'static str,
    had_previous: bool,
}

fn rollback_swaps(live: &Path, saved: &Path, swaps: &[SwappedTarget]) -> bool {
    let mut ok = true;
    for swap in swaps.iter().rev() {
        let target = live.join(swap.name);
        if remove_path(&target).is_err() {
            ok = false;
        }
        if swap.had_previous && fs::rename(saved.join(swap.name), &target).is_err() {
            ok = false;
        }
    }
    ok
}

fn swap_staged(
    live: &Path,
    staged: &Path,
    saved: &Path,
) -> Result<Vec<SwappedTarget>, BackupError> {
    fs::create_dir_all(saved)
        .map_err(|_| error("Não foi possível preparar a cópia de segurança da restauração."))?;
    let mut swaps = Vec::new();
    for name in [
        "system.json",
        "agents.json",
        "workflow-catalog.json",
        "skills.json",
        "skills",
    ] {
        let target = live.join(name);
        let replacement = staged.join(name);
        let had_previous = match fs::symlink_metadata(&target) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    let _ = rollback_swaps(live, saved, &swaps);
                    return Err(error(
                        "Uma configuração atual é um atalho e não pode ser substituída com segurança.",
                    ));
                }
                fs::rename(&target, saved.join(name)).map_err(|_| {
                    let _ = rollback_swaps(live, saved, &swaps);
                    error("Não foi possível preservar as configurações atuais.")
                })?;
                true
            }
            Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => {
                let _ = rollback_swaps(live, saved, &swaps);
                return Err(error("Não foi possível verificar as configurações atuais."));
            }
        };
        if let Err(cause) = fs::rename(&replacement, &target) {
            if had_previous {
                let _ = fs::rename(saved.join(name), &target);
            }
            let rolled_back = rollback_swaps(live, saved, &swaps);
            return Err(error(if rolled_back {
                format!("Não foi possível aplicar a restauração: {cause}")
            } else {
                "A restauração falhou e as configurações anteriores não puderam ser recuperadas automaticamente. Reinicie o Jarvis antes de continuar.".into()
            }));
        }
        swaps.push(SwappedTarget { name, had_previous });
    }
    Ok(swaps)
}

fn prepare_import(
    loaded: &LoadedBackup,
    home: &Path,
    state: &AppState,
    oauth: &OpenAiCodexState,
    mappings: Vec<ModelMapping>,
) -> Result<
    (
        workflow::catalog::Catalog,
        workflow::settings::ModelSettings,
        Vec<String>,
    ),
    BackupError,
> {
    let targets: BTreeMap<_, _> = loaded
        .payload
        .model_targets
        .iter()
        .map(|target| (target.id.as_str(), target))
        .collect();
    let mut selected = BTreeMap::new();
    for mapping in mappings {
        if !targets.contains_key(mapping.target_id.as_str())
            || selected.contains_key(&mapping.target_id)
        {
            return Err(error("O mapeamento de modelos é inválido ou repetido."));
        }
        oauth
            .inference_model(
                state,
                home,
                &mapping.choice.account,
                &mapping.choice.model,
                mapping.choice.reasoning.as_deref(),
            )
            .map_err(|cause| error(cause.message))?;
        selected.insert(mapping.target_id, mapping.choice);
    }

    let current = workflow::catalog::read(home)
        .map_err(|_| error("Não foi possível ler o catálogo atual de agentes."))?;
    let mut catalog = loaded.payload.catalog.clone();
    catalog.revision = current
        .revision
        .checked_add(1)
        .ok_or_else(|| error("A revisão do catálogo de agentes atingiu o limite."))?;
    let mut native = BTreeMap::new();
    for (target, choice) in selected {
        if let Some(key) = target.strip_prefix("builtin:") {
            native.insert(key.to_owned(), choice);
        } else if let Some(id) = target.strip_prefix("custom:") {
            let agent = catalog
                .agents
                .iter_mut()
                .find(|agent| agent.id == id)
                .ok_or_else(|| error("Um agente do mapeamento não existe no backup."))?;
            agent.model = Some(choice);
        }
    }
    catalog
        .validate()
        .map_err(|_| error("O catálogo restaurado não passou pela validação."))?;

    let mut bindings: BTreeSet<String> = loaded
        .payload
        .model_targets
        .iter()
        .map(|target| target.id.clone())
        .collect();
    bindings.extend(
        current
            .agents
            .iter()
            .map(|agent| format!("custom:{}", agent.id)),
    );
    for key in [
        "standard/builder",
        "designer/designer",
        "planned/planner",
        "planned/builder",
        "planned/designer",
        "complete/planner",
        "complete/investigator",
        "complete/writer",
        "complete/orchestrator",
        "complete/designer",
        "complete/builder",
        "complete/reviewer",
        "publication/github",
    ] {
        bindings.insert(format!("builtin:{key}"));
    }
    Ok((catalog, native, bindings.into_iter().collect()))
}

fn apply_import(
    app: &tauri::AppHandle,
    home: &Path,
    state: &AppState,
    mcp: &McpState,
    oauth: &OpenAiCodexState,
    loaded: LoadedBackup,
    mappings: Vec<ModelMapping>,
) -> Result<BackupImportResult, BackupError> {
    let (catalog, native, bindings) = prepare_import(&loaded, home, state, oauth, mappings)?;
    let jarvis = crate::data_dir::root(home);
    fs::create_dir_all(&jarvis)
        .map_err(|_| error("Não foi possível acessar a pasta de configuração do Jarvis."))?;
    let staging = tempfile::Builder::new()
        .prefix("settings-restore-")
        .tempdir_in(&jarvis)
        .map_err(|_| error("Não foi possível preparar a restauração."))?;
    let staged = staging.path().join("next");
    let saved = staging.path().join("previous");
    fs::create_dir_all(staged.join("skills"))
        .map_err(|_| error("Não foi possível preparar as skills para restauração."))?;
    write_staged_file(
        &staged.join("system.json"),
        &serde_json::to_vec_pretty(&loaded.payload.system)
            .map_err(|_| error("Não foi possível preparar as preferências."))?,
    )?;
    write_staged_file(
        &staged.join("agents.json"),
        &serde_json::to_vec_pretty(&native)
            .map_err(|_| error("Não foi possível preparar os agentes."))?,
    )?;
    write_staged_file(
        &staged.join("workflow-catalog.json"),
        &serde_json::to_vec_pretty(&catalog)
            .map_err(|_| error("Não foi possível preparar os fluxos."))?,
    )?;
    write_staged_file(
        &staged.join("skills.json"),
        &serde_json::to_vec_pretty(
            &skills::restore_config(home, &loaded.payload.skills)
                .map_err(|cause| error(cause.message))?,
        )
        .map_err(|_| error("Não foi possível preparar as skills."))?,
    )?;
    for file in &loaded.skill_files {
        let destination = staged.join("skills").join(&file.path);
        write_staged_file(&destination, &file.bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&destination, fs::Permissions::from_mode(file.mode))
                .map_err(|_| error("Não foi possível restaurar as permissões de uma skill."))?;
        }
    }

    let swaps = swap_staged(&jarvis, &staged, &saved)?;
    #[cfg(unix)]
    if fs::File::open(&jarvis)
        .and_then(|directory| directory.sync_all())
        .is_err()
    {
        let recovered = rollback_swaps(&jarvis, &saved, &swaps);
        return Err(error(if recovered {
            "Não foi possível sincronizar as configurações restauradas em disco."
        } else {
            "A restauração não pôde ser sincronizada nem revertida. Reinicie o Jarvis."
        }));
    }
    let system_state = app.state::<system::SystemState>();
    if let Err(cause) = system_state.reload_from_disk(app, home) {
        let recovered = rollback_swaps(&jarvis, &saved, &swaps);
        if recovered {
            let _ = system_state.reload_from_disk(app, home);
        }
        return Err(error(if recovered {
            cause
        } else {
            "As preferências não puderam ser recarregadas e a restauração anterior falhou. Reinicie o Jarvis.".into()
        }));
    }
    if let Err(cause) = mcp.replace_from_backup(state, home, &loaded.payload.mcps, &bindings) {
        let recovered = rollback_swaps(&jarvis, &saved, &swaps);
        if recovered {
            let _ = system_state.reload_from_disk(app, home);
        }
        return Err(error(if recovered {
            cause.message
        } else {
            "Os MCPs não puderam ser restaurados e as configurações anteriores exigem recuperação manual. Reinicie o Jarvis.".into()
        }));
    }

    let mapped_models = native.len()
        + catalog
            .agents
            .iter()
            .filter(|agent| agent.model.is_some())
            .count();
    Ok(BackupImportResult {
        summary: loaded.manifest.summary,
        mapped_models,
    })
}

fn archive_path(raw: &str) -> Result<PathBuf, BackupError> {
    let path = PathBuf::from(raw);
    if raw.trim().is_empty() || !path.is_absolute() {
        return Err(error(
            "Escolha um caminho absoluto para o arquivo de backup.",
        ));
    }
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("zip"))
    {
        return Err(error("O arquivo de backup deve usar a extensão .zip."));
    }
    Ok(path)
}

#[tauri::command]
pub async fn export_settings_backup(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    mcp: tauri::State<'_, McpState>,
    path: String,
) -> Result<BackupExportResult, BackupError> {
    let home = app
        .path()
        .home_dir()
        .map_err(|_| error("Pasta pessoal indisponível."))?;
    let state = state.inner().clone();
    let mcp = mcp.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lock = LOCK.lock().map_err(|_| error("Backup ocupado."))?;
        let _skills = skills::lock().map_err(|cause| error(cause.message))?;
        let path = archive_path(&path)?;
        let system = system::backup_preferences(&home).map_err(error)?;
        let catalog = clean_catalog(
            workflow::catalog::read(&home)
                .map_err(|_| error("Não foi possível ler os agentes e fluxos."))?,
        );
        let native = workflow::settings::read(&home)
            .map_err(|_| error("Não foi possível ler os modelos dos agentes."))?;
        let payload = SettingsPayload {
            system,
            model_targets: model_targets(&native, &catalog)?,
            catalog,
            skills: skills::backup_config(&home).map_err(|cause| error(cause.message))?,
            mcps: mcp
                .backup_configs(&state, &home)
                .map_err(|cause| error(cause.message))?,
        };
        validate_payload(&payload)?;
        let files = collect_skill_files(&skills::root(&home).join("skills"))?;
        let result_summary = summary(&payload, &files);
        let bytes = write_archive(&path, &payload, &files)?;
        Ok(BackupExportResult {
            path: path.to_string_lossy().into_owned(),
            bytes,
            summary: result_summary,
        })
    })
    .await
    .map_err(|_| error("A criação do backup foi interrompida."))?
}

#[tauri::command]
pub async fn inspect_settings_backup(path: String) -> Result<BackupPreview, BackupError> {
    tauri::async_runtime::spawn_blocking(move || {
        let _lock = LOCK.lock().map_err(|_| error("Backup ocupado."))?;
        read_archive(&archive_path(&path)?).map(|loaded| preview(&loaded))
    })
    .await
    .map_err(|_| error("A inspeção do backup foi interrompida."))?
}

#[tauri::command]
pub async fn import_settings_backup(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    mcp: tauri::State<'_, McpState>,
    oauth: tauri::State<'_, OpenAiCodexState>,
    path: String,
    fingerprint: String,
    mappings: Vec<ModelMapping>,
) -> Result<BackupImportResult, BackupError> {
    if app.state::<agent::AgentState>().has_active_chats() {
        return Err(error(
            "Aguarde os chats e agentes terminarem antes de restaurar um backup.",
        ));
    }
    let home = app
        .path()
        .home_dir()
        .map_err(|_| error("Pasta pessoal indisponível."))?;
    let state = state.inner().clone();
    let mcp = mcp.inner().clone();
    let oauth = oauth.inner().clone();
    let worker_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let _lock = LOCK.lock().map_err(|_| error("Backup ocupado."))?;
        let _skills = skills::lock().map_err(|cause| error(cause.message))?;
        if worker_app.state::<agent::AgentState>().has_active_chats() {
            return Err(error(
                "Um chat ou agente começou a trabalhar. Aguarde antes de restaurar.",
            ));
        }
        let loaded = read_archive(&archive_path(&path)?)?;
        if loaded.fingerprint != fingerprint {
            return Err(error(
                "O arquivo de backup mudou desde a inspeção. Selecione-o novamente.",
            ));
        }
        apply_import(&worker_app, &home, &state, &mcp, &oauth, loaded, mappings)
    })
    .await
    .map_err(|_| error("A restauração do backup foi interrompida."))??;
    let _ = app.emit("agent-models:changed", ());
    let _ = app.emit("workflow-catalog:changed", ());
    let _ = app.emit("skills:changed", ());
    let _ = app.emit("mcp:changed", ());
    Ok(result)
}

#[cfg(test)]
mod tests;
