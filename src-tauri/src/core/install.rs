use super::*;
use base64::Engine;
use serde_json::Value;
use sha2::{Digest, Sha256, Sha512};
use std::{
    io::{Cursor, Read},
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const NODE_VERSION: &str = "22.23.2";
const DOWNLOAD_LIMIT: usize = 180 * 1024 * 1024;
#[derive(Deserialize)]
pub(super) struct Release {
    tag_name: String,
    assets: Vec<Asset>,
    draft: bool,
    prerelease: bool,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}
impl Release {
    pub fn version(&self) -> String {
        self.tag_name
            .strip_prefix("bun-v")
            .or_else(|| self.tag_name.strip_prefix("open-design-v"))
            .or_else(|| self.tag_name.strip_prefix("@upstash/context7-mcp@"))
            .unwrap_or_else(|| self.tag_name.trim_start_matches('v'))
            .into()
    }
}
fn client() -> Result<reqwest::Client, CoreError> {
    reqwest::Client::builder()
        .user_agent("Jarvis-Core/0.1")
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(240))
        .build()
        .map_err(|_| error("Não foi possível iniciar o download."))
}
async fn download(url: &str, limit: usize) -> Result<Vec<u8>, CoreError> {
    download_with_progress(url, limit, &|_| {}).await
}
// Report the first and last byte counts, and at most four intermediate updates
// per second. Unknown lengths stay unknown instead of showing a fake percentage.
struct DownloadReporter<'a, F: Fn(DownloadProgress)> {
    report: &'a F,
    total: Option<u64>,
    last_update: Instant,
}
impl<'a, F: Fn(DownloadProgress)> DownloadReporter<'a, F> {
    fn new(report: &'a F, total: Option<u64>) -> Self {
        let mut reporter = Self {
            report,
            total: total.filter(|n| *n > 0),
            last_update: Instant::now(),
        };
        reporter.update(0, true);
        reporter
    }
    fn update(&mut self, received: u64, force: bool) {
        if self.total.is_some_and(|total| received > total) {
            self.total = None;
        }
        if force || self.last_update.elapsed() >= Duration::from_millis(250) {
            (self.report)(DownloadProgress {
                received_bytes: received,
                total_bytes: self.total,
            });
            self.last_update = Instant::now();
        }
    }
}
async fn download_with_progress(
    url: &str,
    limit: usize,
    progress: &(impl Fn(DownloadProgress) + Sync),
) -> Result<Vec<u8>, CoreError> {
    let mut response = client()?
        .get(url)
        .send()
        .await
        .map_err(|_| error("Falha de conexão ao baixar o Core. Tente novamente."))?;
    if !response.status().is_success() {
        return Err(error(format!(
            "Download indisponível (HTTP {}). Tente novamente mais tarde.",
            response.status().as_u16()
        )));
    }
    if response
        .content_length()
        .is_some_and(|len| len > limit as u64)
    {
        return Err(error("Download excedeu o tamanho permitido."));
    }
    let mut reporter = DownloadReporter::new(progress, response.content_length());
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| error("Download interrompido. Tente novamente."))?
    {
        if bytes.len() + chunk.len() > limit {
            return Err(error("Download excedeu o tamanho permitido."));
        }
        bytes.extend_from_slice(&chunk);
        reporter.update(bytes.len() as u64, false);
    }
    reporter.update(bytes.len() as u64, true);
    Ok(bytes)
}
async fn json(url: &str) -> Result<Value, CoreError> {
    serde_json::from_slice(&download(url, 4 * 1024 * 1024).await?)
        .map_err(|_| error("Resposta de versão inválida."))
}
// Source releases contain media unrelated to Jarvis. Spool the bounded archive
// to disk instead of retaining hundreds of MB while building the resource index.
async fn source_archive(
    url: &str,
    directory: &Path,
    progress: &(impl Fn(DownloadProgress) + Sync),
) -> Result<(tempfile::NamedTempFile, String), CoreError> {
    const LIMIT: u64 = 512 * 1024 * 1024;
    let mut response = client()?
        .get(url)
        .timeout(Duration::from_secs(600))
        .send()
        .await
        .map_err(|_| error("Falha ao baixar os recursos de design."))?;
    if !response.status().is_success() || response.content_length().is_some_and(|n| n > LIMIT) {
        return Err(error(
            "Arquivo de recursos indisponível ou maior que 512 MB.",
        ));
    }
    let mut reporter = DownloadReporter::new(progress, response.content_length());
    let file = tempfile::NamedTempFile::new_in(directory)?;
    let mut output = tokio::fs::File::from_std(file.reopen()?);
    let mut digest = Sha256::new();
    let mut total = 0u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| error("Download interrompido. Tente novamente."))?
    {
        total += chunk.len() as u64;
        if total > LIMIT {
            return Err(error("Download de recursos excedeu 512 MB."));
        }
        digest.update(&chunk);
        output.write_all(&chunk).await?;
        reporter.update(total, false);
    }
    output.flush().await?;
    reporter.update(total, true);
    Ok((file, format!("{:x}", digest.finalize())))
}
pub(super) async fn release(repository: &str) -> Result<Release, CoreError> {
    let release: Release = serde_json::from_value(
        json(&format!(
            "https://api.github.com/repos/{repository}/releases/latest"
        ))
        .await?,
    )
    .map_err(|_| error("O GitHub não retornou uma release válida."))?;
    let version = semver::Version::parse(&release.version())
        .map_err(|_| error("A release não possui uma versão válida."))?;
    if release.draft || release.prerelease || !version.pre.is_empty() {
        return Err(error("Nenhuma release estável disponível."));
    }
    Ok(release)
}
fn context7_release(releases: Vec<Release>) -> Result<Release, CoreError> {
    releases
        .into_iter()
        .filter(|r| !r.draft && !r.prerelease && r.tag_name.starts_with("@upstash/context7-mcp@"))
        .filter_map(|r| {
            semver::Version::parse(&r.version())
                .ok()
                .filter(|v| v.pre.is_empty())
                .map(|v| (v, r))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, r)| r)
        .ok_or_else(|| error("Nenhuma release estável do Context7 MCP disponível."))
}
pub(super) async fn component_release(id: ComponentId) -> Result<Release, CoreError> {
    if id != ComponentId::Context7 {
        return release(id.repository()).await;
    }
    let releases = serde_json::from_value(
        json("https://api.github.com/repos/upstash/context7/releases?per_page=100").await?,
    )
    .map_err(|_| error("O GitHub não retornou releases válidas do Context7."))?;
    context7_release(releases)
}
fn platform() -> Result<(&'static str, &'static str), CoreError> {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        "linux" => "linux",
        "windows" => "windows",
        _ => {
            return Err(error(
                "O Core ainda não oferece binários para este sistema.",
            ))
        }
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        _ => {
            return Err(error(
                "O Core ainda não oferece binários para esta arquitetura.",
            ))
        }
    };
    Ok((os, arch))
}
pub fn node_path(package: &Path) -> PathBuf {
    package.join(if cfg!(windows) {
        "runtime/node.exe"
    } else {
        "runtime/bin/node"
    })
}
fn npm_path(package: &Path) -> PathBuf {
    package.join(if cfg!(windows) {
        "runtime/node_modules/npm/bin/npm-cli.js"
    } else {
        "runtime/lib/node_modules/npm/bin/npm-cli.js"
    })
}
pub fn executable(name: &str) -> String {
    format!("{name}{}", if cfg!(windows) { ".exe" } else { "" })
}
fn check_hash(bytes: &[u8], digest: &str) -> Result<(), CoreError> {
    let actual = format!("sha256:{:x}", Sha256::digest(bytes));
    if actual != digest {
        return Err(error(
            "A verificação de integridade falhou. Nada foi instalado.",
        ));
    }
    Ok(())
}
pub(super) fn safe_entry(path: &Path, strip: bool) -> Result<PathBuf, CoreError> {
    if path
        .components()
        .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(error("O arquivo contém um caminho inseguro."));
    }
    let mut parts = path
        .components()
        .filter(|c| matches!(c, Component::Normal(_)));
    if strip {
        parts.next();
    }
    Ok(parts.collect())
}
fn unpack(bytes: Vec<u8>, destination: &Path, zip: bool, strip: bool) -> Result<(), CoreError> {
    fs::create_dir_all(destination)?;
    let mut total: u64 = 0;
    if zip {
        let mut archive =
            zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| error("Arquivo ZIP inválido."))?;
        for i in 0..archive.len() {
            let mut item = archive
                .by_index(i)
                .map_err(|_| error("Arquivo ZIP inválido."))?;
            if item.is_symlink() {
                continue;
            }
            let relative = safe_entry(Path::new(item.name()), strip)?;
            if relative.as_os_str().is_empty() {
                continue;
            }
            let path = destination.join(relative);
            if item.is_dir() {
                fs::create_dir_all(path)?;
                continue;
            }
            total += item.size();
            if total > 1024 * 1024 * 1024 {
                return Err(error("Arquivo expandido excedeu o limite."));
            }
            fs::create_dir_all(path.parent().ok_or_else(|| error("Caminho inválido."))?)?;
            let mut file = fs::File::create(&path)?;
            let size = item.size();
            std::io::copy(&mut item.by_ref().take(size), &mut file)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(
                    path,
                    fs::Permissions::from_mode(item.unix_mode().unwrap_or(0o644) & 0o777),
                )?;
            }
        }
    } else {
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(Cursor::new(bytes)));
        for item in archive.entries()? {
            let mut item = item?;
            let relative = safe_entry(&item.path()?, strip)?;
            if relative.as_os_str().is_empty() {
                continue;
            }
            let path = destination.join(relative);
            let kind = item.header().entry_type();
            if kind.is_dir() {
                fs::create_dir_all(path)?;
                continue;
            }
            // Runtime bin aliases are not needed: Jarvis invokes Node and npm by absolute path.
            if !kind.is_file() {
                continue;
            }
            total += item.size();
            if total > 1024 * 1024 * 1024 {
                return Err(error("Arquivo expandido excedeu o limite."));
            }
            fs::create_dir_all(path.parent().ok_or_else(|| error("Caminho inválido."))?)?;
            let mut file = fs::File::create(&path)?;
            std::io::copy(&mut item, &mut file)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(
                    path,
                    fs::Permissions::from_mode(item.header().mode()? & 0o777),
                )?;
            }
        }
    }
    Ok(())
}
async fn extract(
    bytes: Vec<u8>,
    destination: PathBuf,
    zip: bool,
    strip: bool,
) -> Result<(), CoreError> {
    tokio::task::spawn_blocking(move || unpack(bytes, &destination, zip, strip))
        .await
        .map_err(|_| error("Falha ao extrair o Core."))?
}
pub(super) async fn command(
    mut cmd: tokio::process::Command,
    seconds: u64,
) -> Result<String, CoreError> {
    command_input(&mut cmd, seconds, None).await
}
pub(super) async fn command_input(
    cmd: &mut tokio::process::Command,
    seconds: u64,
    input: Option<Vec<u8>>,
) -> Result<String, CoreError> {
    crate::background::prepare_node(cmd)?;
    cmd.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);
    // A timeout must terminate npm and its descendants, not just the parent.
    let mut wrapped = process_wrap::tokio::CommandWrap::from(std::mem::replace(
        cmd,
        tokio::process::Command::new("unused"),
    ));
    #[cfg(unix)]
    wrapped.wrap(process_wrap::tokio::ProcessGroup::leader());
    #[cfg(windows)]
    crate::background::windows_job(&mut wrapped);
    wrapped.wrap(process_wrap::tokio::KillOnDrop);
    let mut child = wrapped
        .spawn()
        .map_err(|_| error("Não foi possível iniciar o runtime do Core."))?;
    let stdout = child
        .stdout()
        .take()
        .ok_or_else(|| error("Runtime sem saída."))?;
    let stderr = child
        .stderr()
        .take()
        .ok_or_else(|| error("Runtime sem diagnóstico."))?;
    let mut stdin = child.stdin().take();
    let drain = |stream: Box<dyn tokio::io::AsyncRead + Unpin + Send>| async move {
        let mut stream = stream;
        let mut captured = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let size = stream.read(&mut buffer).await?;
            if size == 0 {
                break;
            }
            if captured.len() < 64_000 {
                captured.extend_from_slice(&buffer[..size.min(64_000 - captured.len())]);
            }
        }
        Ok::<_, std::io::Error>(captured)
    };
    let work = async {
        let write = async {
            if let (Some(mut stdin), Some(input)) = (stdin.take(), input) {
                stdin.write_all(&input).await?;
                stdin.shutdown().await?;
            }
            Ok::<_, std::io::Error>(())
        };
        let (out, _err, status, written) = tokio::join!(
            drain(Box::new(stdout)),
            drain(Box::new(stderr)),
            child.wait(),
            write
        );
        written?;
        if !status?.success() {
            #[cfg(test)]
            eprintln!(
                "Core diagnostic: {}",
                String::from_utf8_lossy(_err.as_deref().unwrap_or_default())
            );
            return Err(error(
                "O runtime não passou na verificação. Tente reinstalar o componente.",
            ));
        }
        Ok(String::from_utf8_lossy(&out?).into_owned())
    };
    tokio::time::timeout(Duration::from_secs(seconds), work)
        .await
        .map_err(|_| error("O Core excedeu o tempo limite. Tente novamente."))?
}
async fn install_node(
    destination: &Path,
    stage: &(impl Fn(&str) + Sync),
    progress: &(impl Fn(DownloadProgress) + Sync),
) -> Result<(), CoreError> {
    let (os, arch) = platform()?;
    let node_os = if os == "windows" { "win" } else { os };
    let node_arch = if arch == "amd64" { "x64" } else { arch };
    let extension = if cfg!(windows) { "zip" } else { "tar.gz" };
    let name = format!("node-v{NODE_VERSION}-{node_os}-{node_arch}.{extension}");
    let base = format!("https://nodejs.org/dist/v{NODE_VERSION}");
    let hashes = String::from_utf8(download(&format!("{base}/SHASUMS256.txt"), 32_000).await?)
        .map_err(|_| error("Checksums do Node inválidos."))?;
    let expected = hashes
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let hash = fields.next()?;
            (fields.next()? == name).then_some(format!("sha256:{hash}"))
        })
        .ok_or_else(|| error("Runtime Node indisponível para este sistema."))?;
    let bytes = download_with_progress(&format!("{base}/{name}"), DOWNLOAD_LIMIT, progress).await?;
    stage("Preparando runtime Node");
    check_hash(&bytes, &expected)?;
    extract(bytes, destination.join("runtime"), cfg!(windows), true).await?;
    let mut cmd = tokio::process::Command::new(node_path(destination));
    cmd.args(["--no-warnings", "-e", "const {DatabaseSync}=require('node:sqlite'); const db=new DatabaseSync(':memory:'); db.exec('CREATE VIRTUAL TABLE probe USING fts5(text)'); console.log(process.versions.node)"]);
    if command(cmd, 20).await?.trim() != NODE_VERSION {
        return Err(error("Versão do runtime divergente."));
    }
    Ok(())
}
async fn registry_package(
    name: &str,
    version: &str,
    progress: &(impl Fn(DownloadProgress) + Sync),
) -> Result<Vec<u8>, CoreError> {
    let metadata = json(&format!("https://registry.npmjs.org/{name}/{version}")).await?;
    if metadata["version"] != version || metadata["name"] != name {
        return Err(error("O pacote não corresponde à release do GitHub."));
    }
    let url = metadata["dist"]["tarball"]
        .as_str()
        .filter(|u| u.starts_with("https://registry.npmjs.org/"))
        .ok_or_else(|| error("Origem do pacote inválida."))?;
    let integrity = metadata["dist"]["integrity"]
        .as_str()
        .and_then(|s| s.strip_prefix("sha512-"))
        .ok_or_else(|| error("O pacote não oferece verificação de integridade."))?;
    let bytes = download_with_progress(url, DOWNLOAD_LIMIT, progress).await?;
    if base64::engine::general_purpose::STANDARD.encode(Sha512::digest(&bytes)) != integrity {
        return Err(error("A integridade do pacote não corresponde à release."));
    }
    Ok(bytes)
}
async fn install_bun(
    destination: &Path,
    stage: &(impl Fn(&str) + Sync),
    progress: &(impl Fn(DownloadProgress) + Sync),
) -> Result<(), CoreError> {
    let release = release("oven-sh/bun").await?;
    let (os, arch) = platform()?;
    let arch = if arch == "arm64" { "aarch64" } else { "x64" };
    let directory = node_path(destination).parent().unwrap().to_path_buf();
    binary(
        &release,
        &format!("bun-{os}-{arch}.zip"),
        directory.clone(),
        true,
        stage,
        progress,
    )
    .await?;
    let mut cmd = tokio::process::Command::new(directory.join(executable("bun")));
    cmd.arg("--version");
    if command(cmd, 20).await?.trim() != release.version() {
        return Err(error("Versão do Bun divergente."));
    }
    Ok(())
}
async fn binary(
    release: &Release,
    name: &str,
    destination: PathBuf,
    strip: bool,
    stage: &(impl Fn(&str) + Sync),
    progress: &(impl Fn(DownloadProgress) + Sync),
) -> Result<(), CoreError> {
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == name)
        .ok_or_else(|| error("A release não oferece binário para este sistema."))?;
    if !asset
        .browser_download_url
        .starts_with("https://github.com/")
    {
        return Err(error("Origem de release inválida."));
    }
    let digest = asset
        .digest
        .as_deref()
        .filter(|d| d.starts_with("sha256:"))
        .ok_or_else(|| error("A release não oferece checksum SHA-256."))?;
    let bytes =
        download_with_progress(&asset.browser_download_url, DOWNLOAD_LIMIT, progress).await?;
    stage("Verificando e extraindo arquivos");
    check_hash(&bytes, digest)?;
    extract(bytes, destination, name.ends_with(".zip"), strip).await
}
async fn publish(source: &Path, target: &Path) -> Result<(), CoreError> {
    #[cfg(windows)]
    {
        for delay in [0, 50, 100, 250, 500, 1_000, 2_000] {
            if delay > 0 {
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
            match fs::rename(source, target) {
                Ok(()) => return Ok(()),
                Err(cause) if matches!(cause.raw_os_error(), Some(5 | 32 | 33)) => {
                    continue;
                }
                Err(_) => return Err(error("Não foi possível publicar a instalação do Core.")),
            }
        }
        Err(error(
            "Um arquivo do Core está em uso. Feche o aplicativo que o utiliza e tente novamente.",
        ))
    }
    #[cfg(not(windows))]
    {
        fs::rename(source, target)?;
        Ok(())
    }
}
pub(super) async fn install(
    home: &Path,
    id: ComponentId,
    stage: impl Fn(&str) + Sync,
    progress: impl Fn(DownloadProgress) + Sync,
) -> Result<String, CoreError> {
    let release = component_release(id).await?;
    let version = release.version();
    let base = root(home).join(id.key());
    fs::create_dir_all(&base)?;
    let staging = tempfile::Builder::new()
        .prefix(".install-")
        .tempdir_in(&base)?;
    let destination = staging.path();
    let mut required = Vec::<String>::new();
    match id {
        ComponentId::Context7 => {
            stage("Baixando runtime Node");
            install_node(destination, &stage, &progress).await?;
            stage("Baixando Context7");
            let bytes = registry_package("@upstash/context7-mcp", &version, &progress).await?;
            fs::write(destination.join("context7.tgz"), bytes)?;
            fs::write(destination.join("package.json"), br#"{"name":"jarvis-core-context7","private":true,"dependencies":{"@upstash/context7-mcp":"file:context7.tgz"}}"#)?;
            fs::write(destination.join("empty.npmrc"), b"")?;
            stage("Instalando dependências");
            let mut cmd = tokio::process::Command::new(node_path(destination));
            cmd.arg(npm_path(destination))
                .args([
                    "install",
                    "--ignore-scripts",
                    "--omit=dev",
                    "--no-audit",
                    "--no-fund",
                    "--package-lock=true",
                    "--global=false",
                    "--workspaces=false",
                    "--registry=https://registry.npmjs.org",
                ])
                .arg("--prefix")
                .arg(destination)
                .current_dir(destination)
                .env("NODE_OPTIONS", "")
                .env("npm_config_cache", root(home).join("cache/npm"))
                .env("npm_config_userconfig", destination.join("empty.npmrc"));
            command(cmd, 240).await?;
            required.extend(
                [
                    "node_modules/@upstash/context7-mcp/dist/index.js",
                    "node_modules/@upstash/context7-mcp/package.json",
                ]
                .map(String::from),
            );
            required.push(
                node_path(destination)
                    .strip_prefix(destination)
                    .unwrap()
                    .to_string_lossy()
                    .into(),
            );
            stage("Validando ferramentas");
            context7::verify(destination).await?;
        }
        ComponentId::OpenDesign => {
            stage("Baixando recursos de design");
            let commit = json(&format!(
                "https://api.github.com/repos/{}/commits/{}",
                id.repository(),
                release.tag_name
            ))
            .await?;
            let sha = commit["sha"]
                .as_str()
                .filter(|sha| sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()))
                .ok_or_else(|| error("Referência do Open Design inválida."))?
                .to_owned();
            let (archive, digest) = source_archive(
                &format!(
                    "https://codeload.github.com/{}/tar.gz/{sha}",
                    id.repository()
                ),
                &base,
                &progress,
            )
            .await?;
            stage("Indexando sistemas, templates e guias");
            let directory = destination.to_path_buf();
            let expected = version.clone();
            tauri::async_runtime::spawn_blocking(move || {
                design::prepare(archive.reopen()?, &directory, &expected, &sha, &digest)
            })
            .await
            .map_err(|_| error("Não foi possível preparar os recursos de design."))??;
            required.extend(["package.json", "LICENSE", "jarvis-design.json"].map(String::from));
        }
        ComponentId::ContextMode => {
            stage("Baixando runtime Node");
            install_node(destination, &stage, &progress).await?;
            stage("Baixando runtime Bun");
            install_bun(destination, &stage, &progress).await?;
            stage("Baixando Context-mode");
            let bytes = registry_package("context-mode", &version, &progress).await?;
            fs::write(destination.join("context-mode.tgz"), bytes)?;
            fs::write(destination.join("package.json"), br#"{"name":"jarvis-core-context-mode","private":true,"dependencies":{"context-mode":"file:context-mode.tgz"}}"#)?;
            // No lifecycle script may edit another agent's global configuration.
            stage("Instalando dependências");
            let mut cmd = tokio::process::Command::new(node_path(destination));
            cmd.arg(npm_path(destination))
                .args([
                    "install",
                    "--ignore-scripts",
                    "--omit=dev",
                    "--no-audit",
                    "--no-fund",
                    "--package-lock=true",
                    "--global=false",
                    "--workspaces=false",
                    "--registry=https://registry.npmjs.org",
                ])
                .arg("--prefix")
                .arg(destination)
                .env("NODE_OPTIONS", "")
                .current_dir(destination)
                .env("npm_config_cache", root(home).join("cache/npm"))
                .env("npm_config_userconfig", destination.join("empty.npmrc"));
            fs::write(destination.join("empty.npmrc"), b"")?;
            command(cmd, 240).await?;
            fs::write(
                destination.join("jarvis-hook.mjs"),
                include_str!("context-hook.mjs"),
            )?;
            required.extend(
                [
                    "node_modules/context-mode/server.bundle.mjs",
                    "node_modules/context-mode/hooks/session-db.bundle.mjs",
                    "node_modules/context-mode/hooks/session-extract.bundle.mjs",
                    "node_modules/context-mode/hooks/session-snapshot.bundle.mjs",
                    "jarvis-hook.mjs",
                ]
                .map(String::from),
            );
            required.push(
                node_path(destination)
                    .strip_prefix(destination)
                    .unwrap()
                    .to_string_lossy()
                    .into(),
            );
            required.push(
                node_path(destination)
                    .parent()
                    .unwrap()
                    .join(executable("bun"))
                    .strip_prefix(destination)
                    .unwrap()
                    .to_string_lossy()
                    .into(),
            );
            stage("Validando ferramentas e hooks");
            context::verify(destination).await?;
        }
        ComponentId::Ponytail => {
            stage("Baixando Ponytail");
            let bytes = download_with_progress(
                &format!(
                    "https://api.github.com/repos/{}/tarball/{}",
                    id.repository(),
                    release.tag_name
                ),
                DOWNLOAD_LIMIT,
                &progress,
            )
            .await?;
            stage("Extraindo Ponytail");
            extract(bytes, destination.to_path_buf(), false, true).await?;
            let package: Value =
                serde_json::from_slice(&fs::read(destination.join("package.json"))?)
                    .map_err(|_| error("Pacote Ponytail inválido."))?;
            if package["version"] != version || package["name"] != "@dietrichgebert/ponytail" {
                return Err(error("Versão do Ponytail divergente."));
            }
            stage("Validando regras do Ponytail");
            ponytail::Ponytail::at(destination, &version)?;
            required.extend(["package.json", "AGENTS.md", ponytail::SKILL_PATH].map(String::from));
        }
        ComponentId::Beads => {
            let (os, arch) = platform()?;
            let ext = if cfg!(windows) { "zip" } else { "tar.gz" };
            stage("Baixando Beads");
            binary(
                &release,
                &format!("beads_{version}_{os}_{arch}.{ext}"),
                destination.to_path_buf(),
                false,
                &stage,
                &progress,
            )
            .await?;
            stage("Baixando Dolt");
            let dolt = release_for_dolt().await?;
            binary(
                &dolt,
                &format!("dolt-{os}-{arch}.{ext}"),
                destination.join("dolt"),
                true,
                &stage,
                &progress,
            )
            .await?;
            required.extend([executable("bd"), format!("dolt/bin/{}", executable("dolt"))]);
            stage("Validando binários");
            for (file, expected) in [(&required[0], &version), (&required[1], &dolt.version())] {
                let mut cmd = tokio::process::Command::new(destination.join(file));
                cmd.arg("version").current_dir(destination);
                if !command(cmd, 30)
                    .await?
                    .split_whitespace()
                    .any(|word| word.trim_start_matches('v') == expected)
                {
                    return Err(error("Versão do binário divergente."));
                }
            }
        }
    }
    for file in &required {
        if !destination.join(file).is_file() {
            return Err(error("A release está incompleta."));
        }
    }
    stage("Concluindo instalação");
    let final_path = base.join(format!(
        "{version}-{}",
        crate::library::new_id().map_err(|_| error("Não foi possível criar a instalação."))?
    ));
    publish(destination, &final_path).await?;
    let record = Installation {
        version: version.clone(),
        directory: final_path
            .strip_prefix(root(home))
            .map_err(|_| error("Pasta do Core inválida."))?
            .to_string_lossy()
            .into(),
        files: required,
    };
    let mut manifest = read_manifest(home)?;
    super::health::save_receipt(home, id, &record)?;
    manifest.installations.insert(id, record);
    save_manifest(home, &manifest)?;
    // Previous generations stay usable by running conversations until a later maintenance pass.
    Ok(version)
}
async fn release_for_dolt() -> Result<Release, CoreError> {
    release("dolthub/dolt").await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn download_server(
        chunked: bool,
        interrupted: bool,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                request.push(socket.read_u8().await.unwrap());
                assert!(request.len() < 8192);
            }
            let header = if chunked {
                "Transfer-Encoding: chunked"
            } else {
                "Content-Length: 12"
            };
            socket
                .write_all(
                    format!("HTTP/1.1 200 OK\r\n{header}\r\nConnection: close\r\n\r\n").as_bytes(),
                )
                .await
                .unwrap();
            socket
                .write_all(if chunked { b"4\r\nabcd\r\n" } else { b"abcd" })
                .await
                .unwrap();
            if interrupted {
                return;
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
            socket
                .write_all(if chunked { b"4\r\nefgh\r\n" } else { b"efgh" })
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(300)).await;
            socket
                .write_all(if chunked {
                    b"4\r\nijkl\r\n0\r\n\r\n"
                } else {
                    b"ijkl"
                })
                .await
                .unwrap();
        });
        (format!("http://{address}/archive"), task)
    }

    #[tokio::test]
    async fn reports_streamed_bytes_with_and_without_length_for_both_download_paths() {
        for archive in [false, true] {
            for chunked in [false, true] {
                let (url, server) = download_server(chunked, false).await;
                let events = Mutex::new(Vec::new());
                let report = |progress| events.lock().unwrap().push(progress);
                let body = if archive {
                    let directory = tempfile::tempdir().unwrap();
                    let (file, digest) = source_archive(&url, directory.path(), &report)
                        .await
                        .unwrap();
                    let bytes = fs::read(file.path()).unwrap();
                    assert_eq!(digest, format!("{:x}", Sha256::digest(&bytes)));
                    bytes
                } else {
                    download_with_progress(&url, 64, &report).await.unwrap()
                };
                server.await.unwrap();
                assert_eq!(body, b"abcdefghijkl");
                let events = events.into_inner().unwrap();
                assert_eq!(events[0].received_bytes, 0);
                assert!(events
                    .iter()
                    .any(|event| event.received_bytes > 0 && event.received_bytes < 12));
                assert_eq!(events.last().unwrap().received_bytes, 12);
                assert!(events
                    .windows(2)
                    .all(|pair| pair[0].received_bytes <= pair[1].received_bytes));
                assert!(events
                    .iter()
                    .all(|event| event.total_bytes == if chunked { None } else { Some(12) }));
            }
        }
    }

    #[tokio::test]
    async fn interrupted_downloads_never_report_completion_and_remove_partial_archives() {
        for archive in [false, true] {
            let (url, server) = download_server(false, true).await;
            let directory = tempfile::tempdir().unwrap();
            let events = Mutex::new(Vec::new());
            let report = |progress| events.lock().unwrap().push(progress);
            let result = if archive {
                source_archive(&url, directory.path(), &report)
                    .await
                    .map(|_| ())
            } else {
                download_with_progress(&url, 64, &report).await.map(|_| ())
            };
            server.await.unwrap();
            assert!(result.is_err());
            assert!(events
                .into_inner()
                .unwrap()
                .iter()
                .all(|event| event.received_bytes < 12));
            assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn throttles_chunk_events_but_always_reports_the_last_count() {
        let events = Mutex::new(Vec::new());
        let report = |progress| events.lock().unwrap().push(progress);
        let mut reporter = DownloadReporter::new(&report, Some(10_000));
        for received in 1..100 {
            reporter.update(received, false);
        }
        reporter.update(100, true);
        let events = events.into_inner().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events.last().unwrap().received_bytes, 100);
    }
    #[tokio::test]
    #[ignore = "Downloads the official release into a disposable isolated Core installation"]
    async fn official_open_design_install() {
        let home = tempfile::tempdir().unwrap();
        let version = install(
            home.path(),
            ComponentId::OpenDesign,
            |stage| eprintln!("{stage}"),
            |_| {},
        )
        .await
        .unwrap();
        let pack = design::Pack::open(home.path()).unwrap();
        let result = pack
            .execute("design_search", &serde_json::json!({"query":""}))
            .unwrap();
        let result: Value = serde_json::from_str(&result).unwrap();
        assert!(result["total"].as_u64().unwrap() > 200);
        eprintln!(
            "Open Design {version}: {} resources verified",
            result["total"]
        );
    }
    #[test]
    fn open_design_release_prefix_has_a_semantic_version() {
        let release = Release {
            tag_name: "open-design-v0.21.1".into(),
            assets: vec![],
            draft: false,
            prerelease: false,
        };
        assert_eq!(release.version(), "0.21.1");
    }
    #[test]
    fn context7_updates_ignore_other_packages_and_prereleases() {
        let releases = serde_json::from_value(serde_json::json!([
            {"tag_name":"@upstash/context7-sdk@99.0.0","assets":[],"draft":false,"prerelease":false},
            {"tag_name":"@upstash/context7-mcp@4.0.5","assets":[],"draft":false,"prerelease":false},
            {"tag_name":"@upstash/context7-mcp@5.0.0-beta.1","assets":[],"draft":false,"prerelease":true},
            {"tag_name":"@upstash/context7-mcp@4.0.4","assets":[],"draft":false,"prerelease":false}
        ])).unwrap();
        assert_eq!(context7_release(releases).unwrap().version(), "4.0.5");
        assert!(context7_release(vec![]).is_err());
    }
    #[tokio::test]
    #[ignore = "Downloads and verifies the official Context7 MCP package with its private Node runtime"]
    async fn official_context7_install() {
        let home = tempfile::tempdir().unwrap();
        let version = install(
            home.path(),
            ComponentId::Context7,
            |stage| eprintln!("{stage}"),
            |_| {},
        )
        .await
        .unwrap();
        let record = installed(home.path(), ComponentId::Context7).unwrap();
        assert_eq!(record.version, version);
        assert!(node_path(&record.path(home.path()).unwrap()).is_file());
        assert!(!context7::configured(home.path()));
        eprintln!("Context7 {version}: official package and documentation tools verified");
    }
    #[test]
    fn rejects_archive_traversal_and_bad_checksums() {
        for path in ["../outside", "/tmp/outside", "package/../../outside"] {
            assert!(safe_entry(Path::new(path), true).is_err());
        }
        assert_eq!(
            safe_entry(Path::new("package/lib/test.js"), true).unwrap(),
            Path::new("lib/test.js")
        );
        assert!(check_hash(b"tampered", "sha256:bad").is_err());
        assert!(check_hash(b"valid", &format!("sha256:{:x}", Sha256::digest(b"valid"))).is_ok());
    }
    #[cfg(windows)]
    #[tokio::test]
    async fn publishes_after_a_transient_directory_lock() {
        use std::{os::windows::fs::OpenOptionsExt, sync::mpsc};

        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        fs::create_dir(&source).unwrap();
        let locked = source.join("locked");
        fs::write(&locked, b"locked").unwrap();
        let (ready, started) = mpsc::channel();
        let releaser = std::thread::spawn(move || {
            let file = fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(&locked)
                .unwrap();
            ready.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(300));
            drop(file);
        });
        started.recv().unwrap();

        publish(&source, &target).await.unwrap();
        releaser.join().unwrap();
        assert!(target.join("locked").is_file());
    }
}
