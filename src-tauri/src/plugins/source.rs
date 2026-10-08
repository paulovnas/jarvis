use super::*;
use std::{
    fs,
    io::{Read, Write},
    process::Stdio,
    time::Duration,
};
use tokio::process::Command;

pub(super) const MAX_PACKAGE_BYTES: u64 = 128 * 1024 * 1024;
pub(super) const MAX_FILES: usize = 10_000;

pub(super) fn marketplace_package(
    value: Option<&Value>,
    root: &Path,
    cursor: bool,
) -> Result<PackageSource> {
    let value = value.ok_or_else(|| error("invalid_source", "Origem do plugin ausente."))?;
    let local = |path: &str| -> Result<PackageSource> {
        let relative = if matches!(path, "." | "./") {
            PathBuf::new()
        } else {
            manifest::relative(path, !cursor)?
        };
        Ok(PackageSource::Local {
            path: root.join(relative).to_string_lossy().into_owned(),
        })
    };
    if let Some(path) = value.as_str() {
        return local(path);
    }
    match value.get("source").and_then(Value::as_str) {
        Some("local") => local(
            value
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| error("invalid_source", "Caminho local ausente."))?,
        ),
        Some("url" | "git-subdir") => {
            let url = value
                .get("url")
                .and_then(Value::as_str)
                .ok_or_else(|| error("invalid_source", "URL Git ausente."))?;
            if matches!(url, "." | "./") && value.get("ref").is_none() && value.get("sha").is_none()
            {
                return local(value.get("path").and_then(Value::as_str).unwrap_or("./"));
            }
            let (url, parsed_ref) = if url.starts_with("./") {
                (
                    root.join(manifest::relative(url, false)?)
                        .to_string_lossy()
                        .into_owned(),
                    None,
                )
            } else {
                git_url(url)?
            };
            let path = value.get("path").and_then(Value::as_str).map(str::to_owned);
            if let Some(path) = &path {
                if manifest::relative(path, false)?.as_os_str().is_empty() {
                    return Err(error("invalid_source", "Use uma subpasta Git válida."));
                }
            }
            let ref_name = value
                .get("ref")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or(parsed_ref);
            validate_ref(ref_name.as_deref())?;
            let sha = value
                .get("sha")
                .and_then(Value::as_str)
                .map(str::to_ascii_lowercase);
            if sha.as_ref().is_some_and(|sha| {
                !(sha.len() >= 40 && sha.len() <= 64 && sha.bytes().all(|c| c.is_ascii_hexdigit()))
            }) {
                return Err(error("invalid_source", "O SHA Git precisa ser completo."));
            }
            Ok(PackageSource::Git {
                url,
                path,
                ref_name,
                sha,
            })
        }
        Some("npm") => {
            let package = value
                .get("package")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let version = value
                .get("version")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let registry = value
                .get("registry")
                .and_then(Value::as_str)
                .map(str::to_owned);
            validate_npm(&package, version.as_deref(), registry.as_deref())?;
            Ok(PackageSource::Npm {
                package,
                version,
                registry,
            })
        }
        _ => Err(error(
            "unsupported_source",
            "A origem deste plugin não é suportada.",
        )),
    }
}

pub(super) fn validate_ref(reference: Option<&str>) -> Result<()> {
    if reference.is_some_and(|r| {
        r.is_empty()
            || r.starts_with('-')
            || r.len() > 256
            || r.chars().any(|c| c.is_control() || c.is_whitespace())
    }) {
        Err(error("invalid_ref", "A referência Git é inválida."))
    } else {
        Ok(())
    }
}

pub(super) fn git_url(input: &str) -> Result<(String, Option<String>)> {
    let input = input.trim();
    let (url, reference) = if let Some((url, reference)) = input.split_once('#') {
        (url, Some(reference.to_owned()))
    } else if !input.contains("://") && !input.starts_with("git@") {
        match input.rsplit_once('@') {
            Some((url, reference)) => (url, Some(reference.to_owned())),
            None => (input, None),
        }
    } else {
        (input, None)
    };
    validate_ref(reference.as_deref())?;
    if url.starts_with("git@") {
        if url.contains(char::is_whitespace) || !url.contains(':') {
            return Err(error("invalid_source", "A origem Git SSH é inválida."));
        }
        return Ok((url.into(), reference));
    }
    if !url.contains("://") {
        let mut parts = url.split('/');
        if let (Some(owner), Some(repo), None) = (parts.next(), parts.next(), parts.next()) {
            if manifest::valid_name(owner) && manifest::valid_name(repo) {
                return Ok((
                    format!(
                        "https://github.com/{owner}/{}.git",
                        repo.trim_end_matches(".git")
                    ),
                    reference,
                ));
            }
        }
        return Err(error(
            "invalid_source",
            "Use proprietário/repositório ou uma URL Git.",
        ));
    }
    let parsed =
        url::Url::parse(url).map_err(|_| error("invalid_source", "A URL Git é inválida."))?;
    if !matches!(parsed.scheme(), "https" | "http" | "ssh")
        || parsed.host_str().is_none()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || (parsed.scheme() != "ssh" && !parsed.username().is_empty())
    {
        return Err(error(
            "invalid_source",
            "A URL Git não pode conter credenciais ou parâmetros.",
        ));
    }
    Ok((url.into(), reference))
}

pub(super) fn local_marketplace(input: &str) -> Option<PathBuf> {
    if input == "."
        || input == ".."
        || input.starts_with("./")
        || input.starts_with("../")
        || Path::new(input).is_absolute()
    {
        Some(PathBuf::from(input))
    } else if let Some(path) = input.strip_prefix("~/") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(path))
    } else {
        None
    }
}

pub(super) async fn clone_git(
    url: &str,
    reference: Option<&str>,
    sha: Option<&str>,
    sparse_paths: &[String],
    destination: &Path,
) -> Result<()> {
    validate_ref(reference)?;
    let mut args = vec![
        "clone".into(),
        "--no-checkout".into(),
        "--filter=blob:none".into(),
    ];
    if !sparse_paths.is_empty() {
        args.push("--sparse".into());
    }
    args.extend([
        "--".into(),
        url.into(),
        destination.to_string_lossy().into_owned(),
    ]);
    command("git", &args, None).await?;
    if !sparse_paths.is_empty() {
        let mut args = vec![
            "sparse-checkout".into(),
            "set".into(),
            "--no-cone".into(),
            "--".into(),
        ];
        for path in sparse_paths {
            manifest::relative(path, false)?;
            args.push(path.clone());
        }
        command("git", &args, Some(destination)).await?;
    }
    let target = sha.or(reference).unwrap_or("HEAD");
    if target.starts_with('-') {
        return Err(error("invalid_ref", "A referência Git é inválida."));
    }
    command(
        "git",
        &[
            "checkout".into(),
            "--detach".into(),
            target.into(),
            "--".into(),
        ],
        Some(destination),
    )
    .await?;
    if let Some(sha) = sha {
        let output = command(
            "git",
            &["rev-parse".into(), "HEAD".into()],
            Some(destination),
        )
        .await?;
        if output.trim() != sha {
            return Err(error(
                "git_integrity",
                "A revisão Git obtida não corresponde ao SHA solicitado.",
            ));
        }
    }
    Ok(())
}

async fn command(program: &str, args: &[String], cwd: Option<&Path>) -> Result<String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command.env("GIT_TERMINAL_PROMPT", "0");
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let output = tokio::time::timeout(Duration::from_secs(120), command.output())
        .await
        .map_err(|_| {
            error(
                "plugin_source_timeout",
                "A obtenção do pacote excedeu o tempo limite; tente novamente.",
            )
        })?
        .map_err(|_| {
            error(
                "plugin_source_unavailable",
                format!("Não foi possível executar {program}; verifique se está instalado."),
            )
        })?;
    if !output.status.success() {
        return Err(error("plugin_source_failed", format!("{program} não conseguiu obter o pacote. Confira a origem e as permissões do repositório.")));
    }
    if output.stdout.len() > 1024 * 1024 {
        return Err(error(
            "plugin_source_failed",
            "A resposta da origem excedeu o limite.",
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn validate_npm(package: &str, version: Option<&str>, registry: Option<&str>) -> Result<()> {
    if package.is_empty()
        || package.len() > 214
        || package.starts_with('-')
        || package
            .bytes()
            .any(|c| !(c.is_ascii_alphanumeric() || b"@/._-".contains(&c)))
        || package.contains("..")
    {
        return Err(error(
            "invalid_npm_source",
            "O nome do pacote npm é inválido.",
        ));
    }
    if version.is_some_and(|v| {
        v.is_empty()
            || v.starts_with('-')
            || v.len() > 128
            || v.bytes()
                .any(|c| !(c.is_ascii_alphanumeric() || b".+_~^<>=|-*".contains(&c)))
    }) {
        return Err(error("invalid_npm_source", "A versão npm é inválida."));
    }
    if let Some(registry) = registry {
        let url = url::Url::parse(registry)
            .map_err(|_| error("invalid_npm_source", "O registry npm é inválido."))?;
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(error(
                "invalid_npm_source",
                "O registry precisa usar HTTPS sem credenciais.",
            ));
        }
    }
    Ok(())
}

pub(super) async fn materialize(source: &PackageSource, directory: &Path) -> Result<PathBuf> {
    match source {
        PackageSource::Local { path } => {
            let path = Path::new(path);
            if path.is_dir() {
                Ok(fs::canonicalize(path).map_err(io_error)?)
            } else {
                fs::create_dir_all(directory).map_err(io_error)?;
                unpack(path, directory)?;
                package_root(directory)
            }
        }
        PackageSource::Git {
            url,
            path,
            ref_name,
            sha,
        } => {
            clone_git(
                url,
                ref_name.as_deref(),
                sha.as_deref(),
                &path.iter().cloned().collect::<Vec<_>>(),
                directory,
            )
            .await?;
            let root = match path {
                Some(path) => directory.join(manifest::relative(path, false)?),
                None => directory.to_owned(),
            };
            Ok(root)
        }
        PackageSource::Npm {
            package,
            version,
            registry,
        } => {
            validate_npm(package, version.as_deref(), registry.as_deref())?;
            fs::create_dir_all(directory).map_err(io_error)?;
            let mut args = vec![
                "pack".into(),
                "--ignore-scripts".into(),
                "--pack-destination".into(),
                directory.to_string_lossy().into_owned(),
            ];
            if let Some(registry) = registry {
                args.extend(["--registry".into(), registry.clone()]);
            }
            args.extend([
                "--".into(),
                version
                    .as_ref()
                    .map_or_else(|| package.clone(), |v| format!("{package}@{v}")),
            ]);
            command(
                if cfg!(windows) { "npm.cmd" } else { "npm" },
                &args,
                Some(directory),
            )
            .await?;
            let archives: Vec<_> = fs::read_dir(directory)
                .map_err(io_error)?
                .filter_map(|entry| entry.ok().map(|e| e.path()))
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("tgz"))
                .collect();
            if archives.len() != 1 {
                return Err(error(
                    "invalid_npm_package",
                    "O npm não retornou um pacote único.",
                ));
            }
            let unpacked = directory.join("unpacked");
            fs::create_dir_all(&unpacked).map_err(io_error)?;
            unpack(&archives[0], &unpacked)?;
            let root = package_root(&unpacked)?;
            let package_json = manifest::document(&root.join("package.json"))?;
            if package_json.get("name").and_then(Value::as_str) != Some(package) {
                return Err(error(
                    "invalid_npm_package",
                    "O pacote npm retornado tem outro nome.",
                ));
            }
            Ok(root)
        }
    }
}

fn package_root(directory: &Path) -> Result<PathBuf> {
    if manifest::parse(directory).is_ok() {
        return Ok(directory.into());
    }
    let entries: Vec<_> = fs::read_dir(directory)
        .map_err(io_error)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();
    if entries.len() == 1 && entries[0].is_dir() && manifest::parse(&entries[0]).is_ok() {
        return Ok(entries[0].clone());
    }
    Err(error(
        "missing_plugin_manifest",
        "O arquivo deve conter um único pacote de plugin compatível.",
    ))
}

pub(super) fn unpack(archive: &Path, destination: &Path) -> Result<()> {
    let file = fs::File::open(archive).map_err(io_error)?;
    if file.metadata().map_err(io_error)?.len() > MAX_PACKAGE_BYTES {
        return Err(error("plugin_too_large", "O pacote excede 128 MiB."));
    }
    let mut total = 0u64;
    let mut count = 0usize;
    if matches!(
        archive.extension().and_then(|e| e.to_str()),
        Some("zip" | "xpi")
    ) {
        let mut zip = zip::ZipArchive::new(file)
            .map_err(|_| error("invalid_archive", "O ZIP é inválido."))?;
        for index in 0..zip.len() {
            let entry = zip
                .by_index(index)
                .map_err(|_| error("invalid_archive", "Não foi possível ler o ZIP."))?;
            let relative = manifest::relative(entry.name(), false)?;
            if entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
            {
                return Err(error(
                    "invalid_archive",
                    "Links não são permitidos em pacotes.",
                ));
            }
            count += 1;
            total = total
                .checked_add(entry.size())
                .ok_or_else(|| error("plugin_too_large", "O pacote é muito grande."))?;
            check_limits(total, count)?;
            let target = destination.join(relative);
            if entry.is_dir() {
                fs::create_dir_all(target).map_err(io_error)?;
            } else {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(io_error)?;
                }
                let mut out = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(target)
                    .map_err(io_error)?;
                std::io::copy(&mut entry.take(MAX_PACKAGE_BYTES + 1), &mut out)
                    .map_err(io_error)?;
            }
        }
    } else {
        let reader: Box<dyn Read> = if matches!(
            archive.extension().and_then(|e| e.to_str()),
            Some("gz" | "tgz")
        ) {
            Box::new(flate2::read::GzDecoder::new(file))
        } else {
            Box::new(file)
        };
        let mut tar = tar::Archive::new(reader);
        for entry in tar.entries().map_err(io_error)? {
            let mut entry = entry.map_err(io_error)?;
            let path = entry
                .path()
                .map_err(io_error)?
                .to_string_lossy()
                .into_owned();
            let relative = manifest::relative(&path, false)?;
            let kind = entry.header().entry_type();
            if !kind.is_file() && !kind.is_dir() {
                return Err(error(
                    "invalid_archive",
                    "Links e arquivos especiais não são permitidos em pacotes.",
                ));
            }
            count += 1;
            total = total
                .checked_add(entry.size())
                .ok_or_else(|| error("plugin_too_large", "O pacote é muito grande."))?;
            check_limits(total, count)?;
            let target = destination.join(relative);
            if kind.is_dir() {
                fs::create_dir_all(target).map_err(io_error)?;
            } else {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(io_error)?;
                }
                let mut out = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target)
                    .map_err(io_error)?;
                std::io::copy(&mut entry, &mut out).map_err(io_error)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(
                        target,
                        fs::Permissions::from_mode(entry.header().mode().unwrap_or(0o644) & 0o777),
                    )
                    .map_err(io_error)?;
                }
            }
        }
    }
    Ok(())
}

pub(super) fn check_limits(total: u64, count: usize) -> Result<()> {
    if total > MAX_PACKAGE_BYTES || count > MAX_FILES {
        Err(error(
            "plugin_too_large",
            "O pacote excede 128 MiB ou 10.000 arquivos.",
        ))
    } else {
        Ok(())
    }
}

pub(super) fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let mut file = fs::File::create(path).map_err(io_error)?;
    file.write_all(&serde_json::to_vec_pretty(value).map_err(json_error)?)
        .map_err(io_error)?;
    Ok(())
}
