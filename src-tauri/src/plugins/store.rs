use super::*;
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    sync::{Arc, OnceLock},
};

const BUILTINS_VERSION: u32 = 1;
const DEFAULT_SOURCES: [(&str, &str, &str, bool); 6] = [
    (
        "openai-curated",
        "OpenAI Plugins",
        "https://github.com/openai/plugins.git",
        true,
    ),
    (
        "firebase",
        "Firebase",
        "https://github.com/firebase/agent-skills.git",
        false,
    ),
    (
        "community-plugins",
        "OpenAI Community",
        "https://github.com/openai/community-plugins.git",
        false,
    ),
    (
        "claude-plugins-official",
        "Anthropic Plugins",
        "https://github.com/anthropics/claude-plugins-official.git",
        false,
    ),
    (
        "superpowers-marketplace",
        "Superpowers Marketplace",
        "https://github.com/obra/superpowers-marketplace.git",
        false,
    ),
    (
        "context-mode",
        "Context-mode",
        "https://github.com/mksglu/context-mode.git",
        false,
    ),
];

fn default_marketplace(id: &str, name: &str, source: &str, protected: bool) -> MarketRecord {
    let sparse_paths = if source == "https://github.com/openai/community-plugins.git" {
        vec![".agents/plugins".into(), "plugins".into()]
    } else {
        Vec::new()
    };
    MarketRecord {
        entry: Marketplace {
            id: id.into(),
            name: name.into(),
            source: source.into(),
            ref_name: None,
            sparse_paths,
            refreshed: false,
            built_in: protected,
        },
        root: None,
        hash: None,
        available: Vec::new(),
        issues: Vec::new(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MarketRecord {
    entry: Marketplace,
    root: Option<PathBuf>,
    hash: Option<String>,
    available: Vec<AvailablePlugin>,
    issues: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PluginRecord {
    entry: InstalledPlugin,
    source: PackageSource,
    parsed: manifest::Parsed,
    #[serde(default)]
    trusted_hashes: Vec<String>,
    #[serde(default)]
    component_settings: BTreeMap<String, bool>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    revision: u64,
    marketplaces: Vec<MarketRecord>,
    installed: Vec<PluginRecord>,
    apps_account_id: Option<String>,
    #[serde(default)]
    builtins_version: u32,
}

impl Default for Stored {
    fn default() -> Self {
        Self {
            revision: 0,
            marketplaces: DEFAULT_SOURCES
                .iter()
                .map(|(id, name, source, protected)| {
                    default_marketplace(id, name, source, *protected)
                })
                .collect(),
            installed: Vec::new(),
            apps_account_id: None,
            builtins_version: BUILTINS_VERSION,
        }
    }
}

#[derive(Debug, Clone)]
struct Stage {
    directory: Arc<tempfile::TempDir>,
    root: PathBuf,
    destination: PathBuf,
    hash: String,
}

/// Prepared changes cannot be deserialized or reconstructed from an approval payload.
#[derive(Debug, Clone)]
pub(crate) struct Prepared {
    pub preview: Preview,
    revision: u64,
    state_hash: String,
    next: Stored,
    stages: Vec<Stage>,
    watched: Vec<(PathBuf, String)>,
    packages_to_verify: Vec<(PathBuf, String)>,
}
impl Prepared {
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }
}

pub(crate) fn catalog_file(home: &Path) -> PathBuf {
    plugin_home(home).join("catalog.json")
}

fn read(home: &Path) -> Result<Stored> {
    let path = catalog_file(home);
    if !path.exists() {
        return Ok(Stored::default());
    }
    let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
    if !metadata.is_file() || metadata.len() > 16 * 1024 * 1024 {
        return Err(error(
            "invalid_plugin_catalog",
            "O catálogo de plugins é inválido.",
        ));
    }
    let bytes = fs::read(path).map_err(io_error)?;
    let mut stored: Stored = serde_json::from_slice(&bytes).map_err(|_| {
        error(
            "invalid_plugin_catalog",
            "O catálogo de plugins está danificado; os pacotes não foram alterados.",
        )
    })?;
    if stored.installed.len() > 512 || stored.marketplaces.len() > 64 {
        return Err(error(
            "invalid_plugin_catalog",
            "O catálogo excede seu limite.",
        ));
    }
    if stored.builtins_version < BUILTINS_VERSION {
        for (id, name, source, protected) in DEFAULT_SOURCES.iter().skip(1) {
            if !stored.marketplaces.iter().any(|marketplace| {
                marketplace.entry.source == *source || marketplace.entry.id == *id
            }) && stored.marketplaces.len() < 64
            {
                stored
                    .marketplaces
                    .push(default_marketplace(id, name, source, *protected));
            }
        }
        stored.builtins_version = BUILTINS_VERSION;
    }
    Ok(stored)
}

fn presentation_root(home: &Path, record: &MarketRecord) -> Option<PathBuf> {
    let root = fs::canonicalize(record.root.as_ref()?).ok()?;
    if let Some(source) = source::local_marketplace(&record.entry.source) {
        return (fs::canonicalize(source).ok()? == root).then_some(root);
    }
    let owned = fs::canonicalize(plugin_home(home).join("marketplaces")).ok()?;
    root.starts_with(owned).then_some(root)
}

fn listings(home: &Path, record: &MarketRecord, with_icons: bool) -> Vec<AvailablePlugin> {
    let mut available = record.available.clone();
    for plugin in &mut available {
        plugin.icon_data_url = None;
    }
    let Some(root) = presentation_root(home, record) else {
        return available;
    };
    let entries = manifest::marketplace_document(&root)
        .ok()
        .and_then(|(document, _)| document.get("plugins").and_then(Value::as_array).cloned())
        .unwrap_or_default();
    let empty = Value::Null;
    for plugin in &mut available {
        let entry = entries
            .iter()
            .find(|entry| entry.get("name").and_then(Value::as_str) == Some(plugin.name.as_str()))
            .unwrap_or(&empty);
        if let PackageSource::Local { path } = &plugin.source {
            if !fs::canonicalize(path).is_ok_and(|path| path.starts_with(&root)) {
                continue;
            }
        }
        // Re-evaluate legacy uppercase product policies while keeping the approved source/identity.
        if !entry.is_null() {
            let products = entry.pointer("/policy/products").and_then(Value::as_array);
            let allowed = products.is_none_or(|products| {
                products.is_empty()
                    || products.iter().filter_map(Value::as_str).any(|product| {
                        product.eq_ignore_ascii_case("codex")
                            || product.eq_ignore_ascii_case("jarvis")
                    })
            });
            plugin.installable = allowed
                && entry
                    .pointer("/policy/installation")
                    .and_then(Value::as_str)
                    != Some("NOT_AVAILABLE");
            if allowed {
                plugin
                    .requirements
                    .retain(|requirement| !requirement.starts_with("Restrito aos produtos:"));
            }
        }
        super::presentation::enrich(plugin, entry, with_icons);
    }
    available
}

fn state_hash(stored: &Stored) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(stored).map_err(json_error)?)
    ))
}

fn owned_root(home: &Path, root: &Path) -> bool {
    let cache = plugin_home(home).join("cache");
    fs::canonicalize(root)
        .ok()
        .zip(fs::canonicalize(cache).ok())
        .is_some_and(|(root, cache)| root != cache && root.starts_with(cache))
}

pub(crate) fn catalog(home: &Path) -> Result<Catalog> {
    catalog_from(home, &read(home)?, false)
}

pub(crate) fn catalog_with_icons(home: &Path) -> Result<Catalog> {
    catalog_from(home, &read(home)?, true)
}

type DiscoveryFailures = BTreeMap<PathBuf, Vec<(String, String)>>;
static DISCOVERY: OnceLock<tokio::sync::Mutex<DiscoveryFailures>> = OnceLock::new();

/// Fetch recommended catalogs once per app session, without installing plugins or trusting hooks.
pub(crate) async fn discover_builtin_catalogs(home: &Path) -> Vec<String> {
    discover_catalogs(
        home,
        &DEFAULT_SOURCES
            .iter()
            .map(|(_, _, url, _)| *url)
            .collect::<Vec<_>>(),
    )
    .await
}

pub(super) async fn discover_catalogs(home: &Path, allowed_sources: &[&str]) -> Vec<String> {
    use futures_util::{stream, StreamExt};
    let key = fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
    // ponytail: One app home needs one startup lock; use per-home locks only if parallel homes are supported.
    let mut attempted = DISCOVERY
        .get_or_init(|| tokio::sync::Mutex::new(BTreeMap::new()))
        .lock()
        .await;
    if !attempted.contains_key(&key) {
        let initial = match read(home) {
            Ok(stored) => stored,
            Err(cause) => return vec![cause.message],
        };
        let entries: Vec<_> = initial
            .marketplaces
            .iter()
            .filter(|record| {
                !record.entry.refreshed
                    && allowed_sources
                        .iter()
                        .any(|source| record.entry.source == *source)
            })
            .map(|record| record.entry.clone())
            .collect();
        let mut failures = Vec::new();
        let mut fetched = stream::iter(entries)
            .map(|entry| async move {
                let source = entry.source.clone();
                let result = match read(home) {
                    Ok(current) => {
                        preview(
                            home,
                            current.revision,
                            Operation::RefreshMarketplace {
                                marketplace_id: Some(entry.id.clone()),
                            },
                        )
                        .await
                    }
                    Err(cause) => Err(cause),
                };
                (entry, source, result)
            })
            .buffer_unordered(2);
        while let Some((entry, source, result)) = fetched.next().await {
            let result = result.and_then(|mut prepared| {
                let current = read(home)?;
                let Some(existing) = current
                    .marketplaces
                    .iter()
                    .find(|record| record.entry.source == source && !record.entry.refreshed)
                else {
                    return Ok(());
                };
                let refreshed = prepared
                    .next
                    .marketplaces
                    .iter()
                    .find(|record| record.entry.source == source && record.entry.refreshed)
                    .cloned()
                    .ok_or_else(|| {
                        error(
                            "invalid_marketplace",
                            "O catálogo obtido não corresponde à origem.",
                        )
                    })?;
                if current.marketplaces.iter().any(|record| {
                    record.entry.id == refreshed.entry.id && record.entry.source != source
                }) {
                    return Err(error(
                        "marketplace_conflict",
                        "O catálogo conflita com outra fonte.",
                    ));
                }
                let old_id = existing.entry.id.clone();
                prepared.revision = current.revision;
                prepared.state_hash = state_hash(&current)?;
                prepared.next = current;
                prepared
                    .next
                    .marketplaces
                    .retain(|record| record.entry.id != old_id);
                prepared.next.marketplaces.push(refreshed);
                apply(home, &prepared).map(|_| ())
            });
            if let Err(cause) = result {
                failures.push((
                    source,
                    format!(
                        "{}: {} Use Atualizar loja para tentar novamente.",
                        entry.name, cause.message
                    ),
                ));
            }
        }
        attempted.insert(key.clone(), failures);
    }
    let current = read(home).ok();
    attempted
        .get(&key)
        .into_iter()
        .flatten()
        .filter(|(source, _)| {
            current.as_ref().is_none_or(|stored| {
                stored
                    .marketplaces
                    .iter()
                    .any(|record| record.entry.source == *source && !record.entry.refreshed)
            })
        })
        .map(|(_, message)| message.clone())
        .collect()
}

fn catalog_from(home: &Path, stored: &Stored, with_icons: bool) -> Result<Catalog> {
    let mut installed = Vec::new();
    let mut issues = Vec::new();
    for record in &stored.installed {
        let mut entry = record.entry.clone();
        entry.icon_data_url = None;
        let root = Path::new(&entry.root_path);
        entry.integrity_valid =
            owned_root(home, root) && fingerprint(root).is_ok_and(|hash| hash == entry.hash);
        if !entry.integrity_valid {
            entry.warnings.push("O conteúdo do pacote mudou ou não está disponível. Atualize/reimporte e revise antes de usar.".into());
            issues.push(format!(
                "{}: integridade do pacote inválida.",
                entry.display_name
            ));
        } else if let Ok(metadata) = super::presentation::load(root) {
            if let Some(display_name) = metadata.display_name {
                entry.display_name = display_name;
            }
            if let Some(description) = metadata.description {
                entry.description = description;
            }
            entry.short_description = metadata.short_description;
            entry.category = entry.category.or(metadata.category);
            if with_icons {
                entry.icon_data_url = super::presentation::icon(root, &metadata.icons);
            }
        }
        for component in &mut entry.components {
            component.trusted = entry.integrity_valid
                && (component.kind != ComponentKind::Hooks
                    || record.trusted_hashes.contains(&entry.hash));
        }
        installed.push(entry);
    }
    for marketplace in &stored.marketplaces {
        issues.extend(marketplace.issues.clone());
    }
    Ok(Catalog {
        revision: stored.revision,
        marketplaces: stored
            .marketplaces
            .iter()
            .map(|m| m.entry.clone())
            .collect(),
        available: stored
            .marketplaces
            .iter()
            .flat_map(|marketplace| listings(home, marketplace, with_icons))
            .collect(),
        installed,
        issues,
        apps_account_id: stored.apps_account_id.clone(),
    })
}

pub(crate) async fn preview(
    home: &Path,
    expected_revision: u64,
    operation: Operation,
) -> Result<Prepared> {
    let stored = read(home)?;
    if stored.revision != expected_revision {
        return Err(error(
            "plugin_revision_conflict",
            "Os plugins mudaram. Atualize a lista e revise novamente.",
        ));
    }
    let mut next = stored.clone();
    for marketplace in &mut next.marketplaces {
        marketplace.available = listings(home, marketplace, false);
    }
    let mut prepared = Prepared {
        preview: Preview {
            title: String::new(),
            description: String::new(),
            source: String::new(),
            hash: String::new(),
            components: Vec::new(),
            commands: Vec::new(),
            requirements: Vec::new(),
            warnings: Vec::new(),
            affected_ids: Vec::new(),
        },
        revision: expected_revision,
        state_hash: state_hash(&stored)?,
        next: next.clone(),
        stages: Vec::new(),
        watched: Vec::new(),
        packages_to_verify: Vec::new(),
    };
    match &operation {
        Operation::AddMarketplace {
            source,
            ref_name,
            sparse_paths,
        } => {
            if next.marketplaces.len() >= 64 {
                return Err(error(
                    "plugin_limit",
                    "O limite de marketplaces foi atingido.",
                ));
            }
            let record = prepare_marketplace(
                home,
                source,
                ref_name.clone(),
                sparse_paths.clone(),
                false,
                &mut prepared.stages,
                &mut prepared.watched,
            )
            .await?;
            if next
                .marketplaces
                .iter()
                .any(|m| m.entry.id == record.entry.id && m.entry.source != record.entry.source)
            {
                return Err(error(
                    "marketplace_conflict",
                    "Já existe um marketplace com este nome e outra origem.",
                ));
            }
            prepared.preview.title = "Adicionar marketplace".into();
            prepared.preview.source = record.entry.source.clone();
            prepared.preview.affected_ids.push(record.entry.id.clone());
            prepared.preview.warnings.extend(record.issues.clone());
            next.marketplaces
                .retain(|m| m.entry.source != record.entry.source);
            next.marketplaces.push(record);
        }
        Operation::RefreshMarketplace { marketplace_id } => {
            prepared.preview.title = "Atualizar marketplace".into();
            let entries: Vec<_> = next
                .marketplaces
                .iter()
                .filter(|m| marketplace_id.as_ref().is_none_or(|id| m.entry.id == *id))
                .map(|m| m.entry.clone())
                .collect();
            if entries.is_empty() {
                return Err(error(
                    "marketplace_not_found",
                    "O marketplace não foi encontrado.",
                ));
            }
            for entry in entries {
                let updated = prepare_marketplace(
                    home,
                    &entry.source,
                    entry.ref_name.clone(),
                    entry.sparse_paths.clone(),
                    entry.built_in,
                    &mut prepared.stages,
                    &mut prepared.watched,
                )
                .await?;
                if next
                    .marketplaces
                    .iter()
                    .any(|m| m.entry.id == updated.entry.id && m.entry.source != entry.source)
                {
                    return Err(error(
                        "marketplace_conflict",
                        "O novo catálogo conflita com outro marketplace.",
                    ));
                }
                prepared.preview.affected_ids.push(updated.entry.id.clone());
                prepared.preview.warnings.extend(updated.issues.clone());
                next.marketplaces.retain(|m| m.entry.id != entry.id);
                next.marketplaces.push(updated);
            }
        }
        Operation::RemoveMarketplace { marketplace_id } => {
            let entry = next
                .marketplaces
                .iter()
                .find(|m| m.entry.id == *marketplace_id)
                .ok_or_else(|| {
                    error("marketplace_not_found", "O marketplace não foi encontrado.")
                })?;
            prepared.preview.title = "Remover marketplace".into();
            prepared.preview.source = entry.entry.source.clone();
            prepared.preview.affected_ids.push(marketplace_id.clone());
            prepared
                .preview
                .warnings
                .push("Os plugins instalados e os dados locais serão preservados.".into());
            next.marketplaces.retain(|m| m.entry.id != *marketplace_id);
        }
        Operation::Install { plugin_id } | Operation::Update { plugin_id } => {
            if matches!(operation, Operation::Update { .. })
                && !next
                    .installed
                    .iter()
                    .any(|plugin| plugin.entry.id == *plugin_id)
            {
                return Err(error(
                    "plugin_not_found",
                    "O plugin ainda não está instalado. Instale antes de atualizar.",
                ));
            }
            let available = next
                .marketplaces
                .iter()
                .flat_map(|m| &m.available)
                .find(|p| p.id == *plugin_id)
                .cloned();
            let (source, marketplace_id, name, requirements, category) =
                if let Some(available) = available {
                    if !available.installable {
                        return Err(error(
                            "plugin_not_available",
                            "O marketplace não permite instalar este plugin neste produto.",
                        ));
                    }
                    (
                        available.source,
                        available.marketplace_id,
                        Some(available.name),
                        available.requirements,
                        available.category,
                    )
                } else if matches!(operation, Operation::Update { .. }) {
                    let record = next
                        .installed
                        .iter()
                        .find(|p| p.entry.id == *plugin_id)
                        .ok_or_else(|| error("plugin_not_found", "O plugin não foi encontrado."))?;
                    (
                        record.source.clone(),
                        record.entry.marketplace_id.clone(),
                        Some(record.entry.name.clone()),
                        Vec::new(),
                        record.entry.category.clone(),
                    )
                } else {
                    return Err(error(
                        "plugin_not_found",
                        "O plugin não está no marketplace. Atualize o catálogo.",
                    ));
                };
            if matches!(operation, Operation::Install { .. })
                && next.installed.iter().any(|p| p.entry.id == *plugin_id)
            {
                return Err(error(
                    "plugin_already_installed",
                    "O plugin já está instalado.",
                ));
            }
            let mut record = prepare_package(
                home,
                source,
                &marketplace_id,
                name.as_deref(),
                &mut prepared.stages,
            )
            .await?;
            record.entry.category = category.or(record.entry.category);
            prepared.preview.title = if matches!(operation, Operation::Update { .. }) {
                "Atualizar plugin"
            } else {
                "Instalar plugin"
            }
            .into();
            prepared.preview.requirements.extend(requirements);
            replace_record(&mut next, record, &mut prepared.preview);
        }
        Operation::Import { path } => {
            let source = PackageSource::Local {
                path: fs::canonicalize(path)
                    .map_err(io_error)?
                    .to_string_lossy()
                    .into_owned(),
            };
            let record = prepare_package(home, source, "local", None, &mut prepared.stages).await?;
            prepared.preview.title = "Importar plugin".into();
            replace_record(&mut next, record, &mut prepared.preview);
        }
        Operation::Create { draft } => {
            validate_draft(draft)?;
            let stage = staging(home)?;
            let root = stage.path().join(&draft.name);
            create_draft(&root, draft)?;
            manifest::migrate_commands(&root)?;
            let mut record = prepare_owned(
                home,
                &root,
                PackageSource::Local {
                    path: String::new(),
                },
                "local",
                Some(&draft.name),
                stage.clone(),
                &mut prepared.stages,
            )?;
            record.source = PackageSource::Local {
                path: record.entry.root_path.clone(),
            };
            prepared.preview.title = "Criar plugin".into();
            replace_record(&mut next, record, &mut prepared.preview);
        }
        Operation::Uninstall { plugin_id } => {
            let record = find_mut(&mut next, plugin_id)?;
            prepared.preview.title = "Desinstalar plugin".into();
            set_package_preview(&mut prepared.preview, record);
            prepared.preview.warnings.push(
                "Os dados persistentes e as versões usadas por turnos ativos serão preservados."
                    .into(),
            );
            next.installed.retain(|p| p.entry.id != *plugin_id);
        }
        Operation::SetEnabled {
            plugin_id,
            enabled,
            project_path,
        } => {
            let record = find_mut(&mut next, plugin_id)?;
            if *enabled {
                prepared.packages_to_verify.push((
                    PathBuf::from(&record.entry.root_path),
                    record.entry.hash.clone(),
                ));
            }
            if let Some(path) = project_path {
                let path = fs::canonicalize(path).map_err(io_error)?;
                if !path.is_dir() {
                    return Err(error(
                        "invalid_plugin_scope",
                        "O projeto deve ser uma pasta existente.",
                    ));
                }
                record
                    .entry
                    .project_overrides
                    .insert(path.to_string_lossy().into_owned(), *enabled);
            } else {
                record.entry.enabled = *enabled;
            }
            prepared.preview.title = if *enabled {
                "Ativar plugin"
            } else {
                "Desativar plugin"
            }
            .into();
            set_package_preview(&mut prepared.preview, record);
        }
        Operation::ConfigureComponent {
            plugin_id,
            component_id,
            enabled,
        } => {
            let record = find_mut(&mut next, plugin_id)?;
            if *enabled {
                prepared.packages_to_verify.push((
                    PathBuf::from(&record.entry.root_path),
                    record.entry.hash.clone(),
                ));
            }
            let component = record
                .entry
                .components
                .iter_mut()
                .find(|c| c.id == *component_id)
                .ok_or_else(|| {
                    error(
                        "plugin_component_not_found",
                        "O componente não foi encontrado.",
                    )
                })?;
            if *enabled && !component.supported {
                return Err(error(
                    "unsupported_plugin_component",
                    "Este componente não é suportado pelo host.",
                ));
            }
            component.enabled = *enabled;
            record
                .component_settings
                .insert(component_id.clone(), *enabled);
            prepared.preview.title = "Configurar componente do plugin".into();
            set_package_preview(&mut prepared.preview, record);
        }
        Operation::TrustHooks { plugin_id, trusted } => {
            let record = find_mut(&mut next, plugin_id)?;
            if !record
                .entry
                .components
                .iter()
                .any(|c| c.kind == ComponentKind::Hooks)
            {
                return Err(error(
                    "plugin_hooks_not_found",
                    "Este plugin não contém hooks.",
                ));
            }
            if *trusted
                && (!owned_root(home, Path::new(&record.entry.root_path))
                    || fingerprint(Path::new(&record.entry.root_path))? != record.entry.hash)
            {
                return Err(error(
                    "plugin_integrity",
                    "O pacote mudou; atualize antes de confiar nos hooks.",
                ));
            }
            if *trusted {
                if !record.trusted_hashes.contains(&record.entry.hash) {
                    record.trusted_hashes.push(record.entry.hash.clone());
                }
            } else {
                record.trusted_hashes.clear();
            }
            if *trusted {
                record.parsed = manifest::parse(Path::new(&record.entry.root_path))?;
                prepared.packages_to_verify.push((
                    PathBuf::from(&record.entry.root_path),
                    record.entry.hash.clone(),
                ));
            }
            prepared.preview.title = if *trusted {
                "Revisar e confiar nos hooks"
            } else {
                "Revogar confiança dos hooks"
            }
            .into();
            set_package_preview(&mut prepared.preview, record);
        }
        Operation::SetAppsAccount { account_id } => {
            if account_id.as_ref().is_some_and(|id| {
                id.is_empty() || id.len() > 128 || id.chars().any(char::is_control)
            }) {
                return Err(error(
                    "invalid_apps_account",
                    "A conta selecionada é inválida.",
                ));
            }
            next.apps_account_id = account_id.clone();
            prepared.preview.title = "Selecionar conta ChatGPT para apps".into();
            prepared.preview.requirements.push(
                "Os apps usam a conta selecionada e exigem conexão real ao serviço ChatGPT.".into(),
            );
        }
    }
    if next.installed.len() > 512 {
        return Err(error(
            "plugin_limit",
            "O limite de plugins instalados foi atingido.",
        ));
    }
    prepared.preview.description = format!("{} componente(s) neste pacote. Nenhum comando do plugin será executado durante a instalação.", prepared.preview.components.len());
    prepared.next = next;
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(&operation).map_err(json_error)?);
    digest.update(serde_json::to_vec(&prepared.next).map_err(json_error)?);
    for stage in &prepared.stages {
        digest.update(stage.hash.as_bytes());
    }
    prepared.preview.hash = format!("{:x}", digest.finalize());
    Ok(prepared)
}

fn find_mut<'a>(stored: &'a mut Stored, id: &str) -> Result<&'a mut PluginRecord> {
    stored
        .installed
        .iter_mut()
        .find(|p| p.entry.id == id)
        .ok_or_else(|| error("plugin_not_found", "O plugin não foi encontrado."))
}

fn replace_record(stored: &mut Stored, mut record: PluginRecord, preview: &mut Preview) {
    if let Some(previous) = stored
        .installed
        .iter()
        .find(|p| p.entry.id == record.entry.id)
    {
        record.entry.enabled = previous.entry.enabled;
        record.entry.project_overrides = previous.entry.project_overrides.clone();
        for component in &mut record.entry.components {
            if let Some(enabled) = previous.component_settings.get(&component.id) {
                component.enabled = *enabled && component.supported;
            } else if let Some(old) = previous
                .entry
                .components
                .iter()
                .find(|c| c.id == component.id)
            {
                component.enabled = old.enabled;
            }
        }
        record.trusted_hashes = previous.trusted_hashes.clone();
        record.component_settings = previous.component_settings.clone();
    }
    set_package_preview(preview, &record);
    stored.installed.retain(|p| p.entry.id != record.entry.id);
    stored.installed.push(record);
}

fn set_package_preview(preview: &mut Preview, record: &PluginRecord) {
    preview.source = match &record.source {
        PackageSource::Local { path } => path.clone(),
        PackageSource::Git { url, .. } => url.clone(),
        PackageSource::Npm { package, .. } => format!("npm:{package}"),
    };
    preview.components = record.entry.components.clone();
    preview.commands = record.parsed.commands.clone();
    preview.warnings.extend(record.parsed.warnings.clone());
    preview.affected_ids.push(record.entry.id.clone());
    if !record.parsed.apps.is_empty() {
        preview
            .requirements
            .push("Apps: conta ChatGPT, acesso ao gateway e autorização de cada conector.".into());
    }
    if !record.parsed.hooks.is_empty() {
        preview.requirements.push(
            "Hooks: confiança explícita nos comandos desta revisão antes de executar.".into(),
        );
    }
}

fn staging(home: &Path) -> Result<Arc<tempfile::TempDir>> {
    let directory = plugin_home(home).join("staging");
    fs::create_dir_all(&directory).map_err(io_error)?;
    Ok(Arc::new(tempfile::tempdir_in(directory).map_err(io_error)?))
}

async fn prepare_package(
    home: &Path,
    source: PackageSource,
    marketplace: &str,
    name: Option<&str>,
    stages: &mut Vec<Stage>,
) -> Result<PluginRecord> {
    let stage = staging(home)?;
    let input = source::materialize(&source, &stage.path().join("source")).await?;
    let input_parsed = manifest::parse(&input)?;
    if name.is_some_and(|name| name != input_parsed.name) {
        return Err(error(
            "plugin_name_mismatch",
            "O nome do pacote difere do anunciado no marketplace.",
        ));
    }
    let before = source_fingerprint(&input)?;
    let root = stage.path().join("owned").join(&input_parsed.name);
    if stage.path().starts_with(&input) {
        return Err(error(
            "invalid_plugin_path",
            "Importe a pasta do pacote, não uma pasta que contenha o armazenamento do Jarvis.",
        ));
    }
    copy_package(&input, &root)?;
    if fingerprint(&root)? != before || source_fingerprint(&input)? != before {
        return Err(error(
            "plugin_source_changed",
            "O pacote mudou durante a cópia; revise novamente.",
        ));
    }
    manifest::migrate_commands(&root)?;
    prepare_owned(home, &root, source, marketplace, name, stage, stages)
}

fn prepare_owned(
    home: &Path,
    root: &Path,
    source: PackageSource,
    marketplace: &str,
    name: Option<&str>,
    directory: Arc<tempfile::TempDir>,
    stages: &mut Vec<Stage>,
) -> Result<PluginRecord> {
    let parsed = manifest::parse(root)?;
    if name.is_some_and(|name| name != parsed.name) {
        return Err(error(
            "plugin_name_mismatch",
            "O pacote contém outro nome de plugin.",
        ));
    }
    let id = format!("{}@{marketplace}", parsed.name);
    let identity = format!("{:x}", Sha256::digest(id.as_bytes()));
    let hash = fingerprint(root)?;
    let mut destination = plugin_home(home).join("cache").join(&identity).join(&hash);
    if destination.exists() && !fingerprint(&destination).is_ok_and(|existing| existing == hash) {
        let suffix = directory
            .path()
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        destination = destination.with_file_name(format!("{hash}-recovered-{suffix}"));
    }
    let data_path = plugin_home(home).join("data").join(identity);
    let mut components = parsed.components.clone();
    for component in &mut components {
        if component.kind == ComponentKind::Mcp {
            component.mcp_server_id = Some(mcp_server_id(&id, &component.name));
        } else if component.kind == ComponentKind::Apps {
            component.mcp_server_id = Some(super::apps::server_id(&id));
        }
    }
    let entry = InstalledPlugin {
        id,
        name: parsed.name.clone(),
        marketplace_id: marketplace.into(),
        display_name: parsed.display_name.clone(),
        description: parsed.description.clone(),
        version: parsed.version.clone(),
        hash: hash.clone(),
        root_path: destination.to_string_lossy().into_owned(),
        data_path: data_path.to_string_lossy().into_owned(),
        enabled: true,
        integrity_valid: true,
        components,
        project_overrides: BTreeMap::new(),
        warnings: parsed.warnings.clone(),
        category: parsed.category.clone(),
        short_description: parsed.short_description.clone(),
        icon_data_url: None,
    };
    stages.push(Stage {
        directory,
        root: root.into(),
        destination,
        hash,
    });
    Ok(PluginRecord {
        entry,
        source,
        parsed,
        trusted_hashes: Vec::new(),
        component_settings: BTreeMap::new(),
    })
}

async fn prepare_marketplace(
    home: &Path,
    input: &str,
    reference: Option<String>,
    sparse: Vec<String>,
    built_in: bool,
    stages: &mut Vec<Stage>,
    watched: &mut Vec<(PathBuf, String)>,
) -> Result<MarketRecord> {
    if sparse.len() > 64 {
        return Err(error(
            "invalid_sparse_paths",
            "Use no máximo 64 caminhos esparsos.",
        ));
    }
    let (root, source, reference, stage) = if let Some(local) = source::local_marketplace(input) {
        if reference.is_some() || !sparse.is_empty() {
            return Err(error(
                "invalid_marketplace",
                "Marketplaces locais não aceitam ref ou caminhos esparsos.",
            ));
        }
        let root = fs::canonicalize(local).map_err(io_error)?;
        if !root.is_dir() {
            return Err(error(
                "invalid_marketplace",
                "Selecione uma pasta de marketplace.",
            ));
        }
        (
            root.clone(),
            root.to_string_lossy().into_owned(),
            None,
            None,
        )
    } else {
        let (url, parsed_ref) = source::git_url(input)?;
        let reference = reference.or(parsed_ref);
        source::validate_ref(reference.as_deref())?;
        let stage = staging(home)?;
        let root = stage.path().join("marketplace");
        source::clone_git(&url, reference.as_deref(), None, &sparse, &root).await?;
        (root, url, reference, Some(stage))
    };
    let (name, mut available, issues) = manifest::marketplace(&root, "pending")?;
    let mut final_root = root.clone();
    let hash;
    if let Some(directory) = stage {
        hash = fingerprint(&root)?;
        final_root = plugin_home(home)
            .join("marketplaces")
            .join(&name)
            .join(&hash);
        for plugin in &mut available {
            if let PackageSource::Local { path } = &mut plugin.source {
                let suffix = Path::new(path)
                    .strip_prefix(&root)
                    .map_err(|_| error("invalid_source", "Origem local fora do marketplace."))?;
                *path = final_root.join(suffix).to_string_lossy().into_owned();
            }
        }
        stages.push(Stage {
            directory,
            root,
            destination: final_root.clone(),
            hash: hash.clone(),
        });
    } else {
        let manifest_path = manifest::MARKETPLACES
            .iter()
            .map(|path| root.join(path))
            .find(|path| path.is_file())
            .ok_or_else(|| error("missing_marketplace", "Manifest ausente."))?;
        hash = format!(
            "{:x}",
            Sha256::digest(fs::read(&manifest_path).map_err(io_error)?)
        );
        watched.push((manifest_path, hash.clone()));
    }
    for plugin in &mut available {
        plugin.id = format!("{}@{name}", plugin.name);
        plugin.marketplace_id = name.clone();
    }
    Ok(MarketRecord {
        entry: Marketplace {
            id: name.clone(),
            name,
            source,
            ref_name: reference,
            sparse_paths: sparse,
            refreshed: true,
            built_in,
        },
        root: Some(final_root),
        hash: Some(hash),
        available,
        issues,
    })
}

pub(crate) fn apply(home: &Path, prepared: &Prepared) -> Result<Catalog> {
    let directory = plugin_home(home);
    fs::create_dir_all(&directory).map_err(io_error)?;
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("catalog.lock"))
        .map_err(io_error)?;
    lock.lock_exclusive().map_err(io_error)?;
    let current = read(home)?;
    if current.revision != prepared.revision || state_hash(&current)? != prepared.state_hash {
        return Err(error(
            "plugin_revision_conflict",
            "Os plugins mudaram após a revisão; solicite uma nova aprovação.",
        ));
    }
    for (path, hash) in &prepared.watched {
        if format!("{:x}", Sha256::digest(fs::read(path).map_err(io_error)?)) != *hash {
            return Err(error(
                "plugin_source_changed",
                "O marketplace local mudou após a revisão.",
            ));
        }
    }
    for (root, hash) in &prepared.packages_to_verify {
        if !owned_root(home, root) || fingerprint(root)? != *hash {
            return Err(error(
                "plugin_integrity",
                "O pacote mudou após a revisão. Atualize e revise novamente.",
            ));
        }
    }
    for stage in &prepared.stages {
        if !stage.root.starts_with(stage.directory.path())
            || fingerprint(&stage.root)? != stage.hash
        {
            return Err(error(
                "plugin_integrity",
                "O conteúdo aprovado mudou. Solicite uma nova revisão.",
            ));
        }
        if !stage.destination.starts_with(&directory) {
            return Err(error("plugin_integrity", "O destino do pacote é inválido."));
        }
        if stage.destination.exists() {
            if fingerprint(&stage.destination)? != stage.hash {
                return Err(error(
                    "plugin_integrity",
                    "Uma versão existente do pacote foi modificada.",
                ));
            }
        } else {
            if let Some(parent) = stage.destination.parent() {
                fs::create_dir_all(parent).map_err(io_error)?;
            }
            fs::rename(&stage.root, &stage.destination).map_err(io_error)?;
        }
    }
    let mut next = prepared.next.clone();
    for marketplace in &mut next.marketplaces {
        for plugin in &mut marketplace.available {
            plugin.icon_data_url = None;
        }
    }
    for plugin in &mut next.installed {
        plugin.entry.icon_data_url = None;
    }
    next.revision = current.revision.checked_add(1).ok_or_else(|| {
        error(
            "plugins_storage",
            "O catálogo excedeu seu limite de revisões.",
        )
    })?;
    for record in &next.installed {
        fs::create_dir_all(&record.entry.data_path).map_err(io_error)?;
    }
    let mut file = tempfile::NamedTempFile::new_in(&directory).map_err(io_error)?;
    let bytes = serde_json::to_vec_pretty(&next).map_err(json_error)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(error(
            "plugin_limit",
            "O catálogo de plugins excede 16 MiB.",
        ));
    }
    file.write_all(&bytes).map_err(io_error)?;
    file.as_file().sync_all().map_err(io_error)?;
    file.persist(catalog_file(home)).map_err(|_| {
        error(
            "plugins_storage",
            "Não foi possível salvar o catálogo; a configuração anterior foi preservada.",
        )
    })?;
    catalog_from(home, &next, false)
}

pub(crate) fn load_active(home: &Path) -> Result<Overlay> {
    load_active_for_project(home, None)
}

/// Explicit revocation applies to frozen turns; package updates alone preserve approved old hashes.
pub(crate) fn hook_source_authorized(
    home: &Path,
    project: Option<&Path>,
    source: &HookSource,
) -> bool {
    let Ok(stored) = read(home) else {
        return false;
    };
    let Some(record) = stored
        .installed
        .iter()
        .find(|record| record.entry.id == source.plugin_id)
    else {
        return false;
    };
    record.trusted_hashes.contains(&source.plugin_hash)
        && frozen_component_authorized(
            home,
            project,
            &source.plugin_id,
            &source.component_id,
            &source.plugin_hash,
            &source.root,
        )
}

/// Validate a held contribution without replacing its version with a later installation.
pub(crate) fn frozen_component_authorized(
    home: &Path,
    project: Option<&Path>,
    plugin_id: &str,
    component_id: &str,
    hash: &str,
    root: &Path,
) -> bool {
    let Ok(stored) = read(home) else {
        return false;
    };
    let Some(record) = stored
        .installed
        .iter()
        .find(|record| record.entry.id == plugin_id)
    else {
        return false;
    };
    let project = project
        .and_then(|path| fs::canonicalize(path).ok())
        .map(|path| path.to_string_lossy().into_owned());
    plugin_enabled(record, project.as_deref())
        && record
            .component_settings
            .get(component_id)
            .copied()
            .unwrap_or(true)
        && frozen_package_intact(home, plugin_id, hash, root)
}

fn plugin_enabled(record: &PluginRecord, project: Option<&str>) -> bool {
    project
        .and_then(|path| record.entry.project_overrides.get(path))
        .copied()
        .unwrap_or(record.entry.enabled)
}

fn frozen_package_intact(home: &Path, plugin_id: &str, hash: &str, root: &Path) -> bool {
    let identity = format!("{:x}", Sha256::digest(plugin_id.as_bytes()));
    let parent = plugin_home(home).join("cache").join(identity);
    let root_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    root.parent()
        .and_then(|path| fs::canonicalize(path).ok())
        .zip(fs::canonicalize(parent).ok())
        .is_some_and(|(root_parent, expected_parent)| root_parent == expected_parent)
        && (root_name == hash || root_name.starts_with(&format!("{hash}-recovered-")))
        && owned_root(home, root)
        && fingerprint(root).is_ok_and(|actual| actual == hash)
}

/// A retained skill path still belongs to an intact, active package component.
pub(crate) fn skill_source_authorized(
    home: &Path,
    project: Option<&Path>,
    plugin_id: &str,
    path: &Path,
) -> bool {
    skill_sources_authorized(home, project, &[(plugin_id, path)])
        .into_iter()
        .next()
        .unwrap_or(false)
}

fn skill_package_root(home: &Path, plugin_id: &str, path: &Path) -> Option<(PathBuf, String)> {
    let identity = format!("{:x}", Sha256::digest(plugin_id.as_bytes()));
    let cache = plugin_home(home).join("cache").join(identity);
    let canonical_cache = fs::canonicalize(&cache).ok()?;
    let canonical_path = fs::canonicalize(path).ok()?;
    let root = canonical_path
        .ancestors()
        .find(|ancestor| ancestor.parent() == Some(canonical_cache.as_path()))?;
    let hash = root.file_name().and_then(|name| name.to_str())?.get(..64)?;
    if !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some((cache.join(root.file_name()?), hash.into()))
}

/// One call checks each frozen package once; no integrity result survives this call.
pub(crate) fn skill_sources_authorized(
    home: &Path,
    project: Option<&Path>,
    sources: &[(&str, &Path)],
) -> Vec<bool> {
    let Ok(stored) = read(home) else {
        return vec![false; sources.len()];
    };
    let project = project
        .and_then(|path| fs::canonicalize(path).ok())
        .map(|path| path.to_string_lossy().into_owned());
    type SkillComponents = Vec<(PathBuf, String)>;
    let mut checked: BTreeMap<(String, PathBuf), Option<SkillComponents>> = BTreeMap::new();
    sources
        .iter()
        .map(|(plugin_id, path)| {
            let Some(record) = stored
                .installed
                .iter()
                .find(|record| record.entry.id == *plugin_id)
            else {
                return false;
            };
            let Some((root, hash)) = skill_package_root(home, plugin_id, path) else {
                return false;
            };
            let skills = checked
                .entry(((*plugin_id).into(), root.clone()))
                .or_insert_with(|| {
                    if !plugin_enabled(record, project.as_deref())
                        || !frozen_package_intact(home, plugin_id, &hash, &root)
                    {
                        return None;
                    }
                    let parsed = manifest::parse(&root).ok()?;
                    Some(
                        parsed
                            .skills
                            .iter()
                            .filter_map(|relative| {
                                let path = fs::canonicalize(
                                    root.join(manifest::relative(relative, true).ok()?),
                                )
                                .ok()?;
                                Some((path, format!("skills:{relative}")))
                            })
                            .collect(),
                    )
                });
            let Ok(path) = fs::canonicalize(path) else {
                return false;
            };
            skills.as_ref().is_some_and(|skills| {
                skills.iter().any(|(root, component)| {
                    path.starts_with(root)
                        && record
                            .component_settings
                            .get(component)
                            .copied()
                            .unwrap_or(true)
                })
            })
        })
        .collect()
}

pub(crate) fn load_active_for_project(home: &Path, project: Option<&Path>) -> Result<Overlay> {
    let stored = read(home)?;
    let catalog = catalog_from(home, &stored, false)?;
    let project = project
        .and_then(|p| fs::canonicalize(p).ok())
        .map(|p| p.to_string_lossy().into_owned());
    let mut overlay = Overlay {
        revision: stored.revision,
        warnings: catalog.issues,
        ..Overlay::default()
    };
    for record in &stored.installed {
        let entry = &record.entry;
        let enabled = project
            .as_ref()
            .and_then(|p| entry.project_overrides.get(p))
            .copied()
            .unwrap_or(entry.enabled);
        if !enabled
            || !catalog
                .installed
                .iter()
                .any(|p| p.id == entry.id && p.integrity_valid)
        {
            continue;
        }
        let root = PathBuf::from(&entry.root_path);
        let data_path = PathBuf::from(&entry.data_path);
        // Contributions are derived from the verified package, never mutable catalog metadata.
        let parsed = manifest::parse(&root)?;
        let allowed = |id: &str| {
            entry
                .components
                .iter()
                .any(|c| c.id == id && c.enabled && c.supported)
        };
        overlay.capabilities.push(PluginCapabilities {
            id: entry.id.clone(),
            name: parsed.display_name.clone(),
            description: parsed.description.chars().take(1024).collect(),
            components: parsed
                .components
                .iter()
                .filter(|c| allowed(&c.id))
                .map(|c| c.id.clone())
                .collect(),
            mcp_servers: parsed
                .mcp
                .keys()
                .filter(|name| allowed(&format!("mcp:{name}")))
                .map(|name| format!("{}: {name}", entry.id))
                .collect(),
        });
        for path in &parsed.skills {
            let id = format!("skills:{path}");
            if allowed(&id) {
                overlay.skill_roots.push(SkillRoot {
                    plugin_id: entry.id.clone(),
                    component_id: id,
                    path: root.join(manifest::relative(path, true)?),
                    recursive: !parsed.portable,
                });
            }
        }
        for (name, definition) in &parsed.mcp {
            let id = format!("mcp:{name}");
            if allowed(&id) {
                overlay.mcp_servers.push(McpContribution {
                    plugin_id: entry.id.clone(),
                    plugin_hash: entry.hash.clone(),
                    component_id: id,
                    name: name.clone(),
                    definition: definition.clone(),
                    root: root.clone(),
                    data_path: data_path.clone(),
                });
            }
        }
        for document in &parsed.hooks {
            let id = format!("hooks:{}", document.name);
            if allowed(&id) {
                overlay.hook_sources.push(HookSource {
                    plugin_id: entry.id.clone(),
                    plugin_hash: entry.hash.clone(),
                    component_id: id,
                    name: document.name.clone(),
                    definition: document.definition.clone(),
                    root: root.clone(),
                    data_path: data_path.clone(),
                    trusted: record.trusted_hashes.contains(&entry.hash),
                });
            }
        }
        for (name, definition) in &parsed.apps {
            let component_id = format!("apps:{name}");
            if allowed(&component_id) {
                overlay.apps.push(AppContribution {
                    plugin_id: entry.id.clone(),
                    plugin_hash: entry.hash.clone(),
                    id: definition
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .into(),
                });
            }
        }
        overlay.warnings.extend(parsed.warnings);
    }
    Ok(overlay)
}

fn files(root: &Path, skip_links: bool) -> Result<Vec<PathBuf>> {
    let metadata = fs::symlink_metadata(root).map_err(io_error)?;
    if !metadata.is_dir() {
        return Err(error(
            "invalid_plugin_path",
            "O pacote deve ser uma pasta regular.",
        ));
    }
    let mut stack = vec![root.to_owned()];
    let mut files = Vec::new();
    let mut count = 0usize;
    let mut total = 0u64;
    while let Some(directory) = stack.pop() {
        if directory
            .strip_prefix(root)
            .map_or(0, |p| p.components().count())
            > 32
        {
            return Err(error(
                "plugin_too_large",
                "O pacote contém pastas profundas demais.",
            ));
        }
        for entry in fs::read_dir(directory).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            if entry.file_name() == ".git" {
                continue;
            }
            let metadata = fs::symlink_metadata(entry.path()).map_err(io_error)?;
            if metadata.file_type().is_symlink() {
                if skip_links {
                    continue;
                }
                return Err(error(
                    "plugin_integrity",
                    "Uma versão instalada contém links inesperados.",
                ));
            }
            count += 1;
            if metadata.is_dir() {
                stack.push(entry.path());
            } else if metadata.is_file() {
                total = total
                    .checked_add(metadata.len())
                    .ok_or_else(|| error("plugin_too_large", "O pacote é muito grande."))?;
                files.push(entry.path());
            } else {
                return Err(error(
                    "invalid_plugin_path",
                    "O pacote contém arquivos especiais.",
                ));
            }
            source::check_limits(total, count)?;
        }
    }
    files.sort();
    Ok(files)
}

fn fingerprint(root: &Path) -> Result<String> {
    tree_fingerprint(root, false)
}

fn source_fingerprint(root: &Path) -> Result<String> {
    tree_fingerprint(root, true)
}

fn tree_fingerprint(root: &Path, skip_links: bool) -> Result<String> {
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 16384];
    for path in files(root, skip_links)? {
        let relative = path
            .strip_prefix(root)
            .map_err(|_| error("invalid_plugin_path", "Caminho fora do pacote."))?
            .to_string_lossy()
            .replace('\\', "/");
        hash.update((relative.len() as u64).to_le_bytes());
        hash.update(relative.as_bytes());
        let size = fs::metadata(&path).map_err(io_error)?.len();
        hash.update(size.to_le_bytes());
        let mut file = fs::File::open(path).map_err(io_error)?;
        let mut seen = 0u64;
        loop {
            let n = file.read(&mut bytes).map_err(io_error)?;
            if n == 0 {
                break;
            }
            seen += n as u64;
            if seen > source::MAX_PACKAGE_BYTES {
                return Err(error(
                    "plugin_too_large",
                    "Um arquivo do pacote excedeu o limite.",
                ));
            }
            hash.update(&bytes[..n]);
        }
        if seen != size {
            return Err(error(
                "plugin_source_changed",
                "Um arquivo mudou durante a revisão.",
            ));
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn copy_package(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination).map_err(io_error)?;
    for path in files(source, true)? {
        let relative = path
            .strip_prefix(source)
            .map_err(|_| error("invalid_plugin_path", "Caminho fora do pacote."))?;
        let target = destination.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        fs::copy(path, target).map_err(io_error)?;
    }
    Ok(())
}

fn validate_draft(draft: &Draft) -> Result<()> {
    if !manifest::valid_name(&draft.name)
        || draft.description.len() > 8192
        || draft.skills.len() > 64
        || draft.files.len() > 128
    {
        return Err(error(
            "invalid_plugin_draft",
            "Nome, descrição ou quantidade de arquivos inválida.",
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut total = 0usize;
    for skill in &draft.skills {
        if !manifest::valid_name(&skill.name)
            || !seen.insert(format!("skills/{}/SKILL.md", skill.name))
            || skill.content.len() > 256 * 1024
        {
            return Err(error(
                "invalid_plugin_draft",
                "Uma skill é inválida, duplicada ou muito grande.",
            ));
        }
        total += skill.content.len();
    }
    for file in &draft.files {
        let path = manifest::relative(&file.path, false)?;
        let path = path.to_string_lossy().replace('\\', "/");
        if path.is_empty()
            || [
                "plugin.json",
                ".codex-plugin/plugin.json",
                ".claude-plugin/plugin.json",
                ".cursor-plugin/plugin.json",
                ".mcp.json",
                ".app.json",
                "hooks/hooks.json",
            ]
            .contains(&path.as_str())
            || !seen.insert(path)
            || file.content.len() > 1024 * 1024
        {
            return Err(error(
                "invalid_plugin_draft",
                "Um arquivo é inválido, duplicado, reservado ou muito grande.",
            ));
        }
        total += file.content.len();
    }
    if total > 16 * 1024 * 1024 {
        return Err(error("invalid_plugin_draft", "O rascunho excede 16 MiB."));
    }
    Ok(())
}

fn create_draft(root: &Path, draft: &Draft) -> Result<()> {
    fs::create_dir_all(root).map_err(io_error)?;
    source::write_json(
        &root.join(".codex-plugin/plugin.json"),
        &serde_json::json!({"name":draft.name,"description":draft.description,"version":"1.0.0"}),
    )?;
    for skill in &draft.skills {
        let path = root.join("skills").join(&skill.name);
        fs::create_dir_all(&path).map_err(io_error)?;
        fs::write(path.join("SKILL.md"), &skill.content).map_err(io_error)?;
    }
    for file in &draft.files {
        let path = root.join(manifest::relative(&file.path, false)?);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        fs::write(path, &file.content).map_err(io_error)?;
    }
    if !draft.mcp_servers.is_empty() {
        source::write_json(
            &root.join(".mcp.json"),
            &serde_json::json!({"mcpServers":draft.mcp_servers}),
        )?;
    }
    if let Some(hooks) = &draft.hooks {
        source::write_json(&root.join("hooks/hooks.json"), hooks)?;
    }
    if !draft.apps.is_empty() {
        source::write_json(
            &root.join(".app.json"),
            &serde_json::json!({"apps":draft.apps}),
        )?;
    }
    Ok(())
}
