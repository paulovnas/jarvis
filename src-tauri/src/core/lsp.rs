//! Jarvis-managed language servers used by native code-navigation tools.

use super::{error, install, installed, ComponentId, CoreError};
use std::path::Path;

pub(super) const TYPESCRIPT_SERVER: &str = "node_modules/typescript-language-server/lib/cli.mjs";
pub(super) const TYPESCRIPT_PACKAGE: &str = "node_modules/typescript/package.json";
pub(super) const TYPESCRIPT_RUNTIME: &str = "node_modules/typescript/lib/tsserver.js";
pub(super) const PYRIGHT_SERVER: &str = "node_modules/pyright/langserver.index.js";
pub(super) const PYRIGHT_CLI: &str = "node_modules/pyright/index.js";
pub(super) const TYPESCRIPT_VERSION: &str = "6.0.3";

/// Project-local servers remain the first choice. This command is the managed
/// fallback for runtimes that can be distributed safely with Jarvis.
pub(crate) fn command(home: &Path, name: &str) -> Option<Vec<String>> {
    let package = installed(home, ComponentId::Lsp).ok()?.path(home).ok()?;
    let entry = match name {
        "typescript-language-server" => TYPESCRIPT_SERVER,
        "pyright-langserver" => PYRIGHT_SERVER,
        _ => return None,
    };
    Some(vec![
        install::node_path(&package).to_string_lossy().into_owned(),
        package.join(entry).to_string_lossy().into_owned(),
    ])
}

pub(super) async fn verify(package: &Path) -> Result<(), CoreError> {
    let node = install::node_path(package);
    for entry in [
        TYPESCRIPT_SERVER,
        TYPESCRIPT_PACKAGE,
        TYPESCRIPT_RUNTIME,
        PYRIGHT_SERVER,
        PYRIGHT_CLI,
    ] {
        if !package.join(entry).is_file() {
            return Err(error(
                "O pacote LSP está incompleto. Reinstale o componente.",
            ));
        }
    }

    let mut typescript_server = tokio::process::Command::new(&node);
    typescript_server
        .arg(package.join(TYPESCRIPT_SERVER))
        .arg("--version")
        .current_dir(package);
    let server_version = install::command(typescript_server, 20).await?;
    semver::Version::parse(server_version.trim())
        .map_err(|_| error("O servidor TypeScript instalado não respondeu corretamente."))?;

    let mut typescript = tokio::process::Command::new(&node);
    typescript
        .args([
            "--no-warnings",
            "-e",
            "const ts=require('typescript'); if(!ts.version) process.exit(1); console.log(ts.version)",
        ])
        .current_dir(package);
    let typescript_version = install::command(typescript, 20).await?;
    if typescript_version.trim() != TYPESCRIPT_VERSION {
        return Err(error(
            "O runtime TypeScript instalado é incompatível. Reinstale o componente.",
        ));
    }

    let mut pyright = tokio::process::Command::new(node);
    pyright
        .arg(package.join(PYRIGHT_CLI))
        .arg("--version")
        .current_dir(package);
    let pyright_version = install::command(pyright, 20).await?;
    semver::Version::parse(
        pyright_version
            .trim()
            .strip_prefix("pyright ")
            .ok_or_else(|| error("O servidor Python instalado não respondeu corretamente."))?,
    )
    .map_err(|_| error("O servidor Python instalado não respondeu corretamente."))?;
    Ok(())
}

pub(super) fn required_files() -> Vec<String> {
    let mut files = [
        TYPESCRIPT_SERVER,
        TYPESCRIPT_PACKAGE,
        TYPESCRIPT_RUNTIME,
        PYRIGHT_SERVER,
        PYRIGHT_CLI,
    ]
    .map(String::from)
    .to_vec();
    files.push(if cfg!(windows) {
        "runtime/node.exe".into()
    } else {
        "runtime/bin/node".into()
    });
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{root, save_manifest, Installation, Manifest};
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn resolves_only_managed_servers_from_a_valid_core_generation() {
        let home = tempfile::tempdir().unwrap();
        let directory = "lsp/fixture";
        let package = root(home.path()).join(directory);
        for file in required_files() {
            let path = package.join(&file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "fixture").unwrap();
        }
        let mut manifest = Manifest::default();
        manifest.installations.insert(
            ComponentId::Lsp,
            Installation {
                version: "1.0.0".into(),
                directory: directory.into(),
                files: required_files(),
            },
        );
        save_manifest(home.path(), &manifest).unwrap();
        let typescript = command(home.path(), "typescript-language-server").unwrap();
        assert_eq!(PathBuf::from(&typescript[0]), install::node_path(&package));
        assert_eq!(
            PathBuf::from(&typescript[1]),
            package.join(TYPESCRIPT_SERVER)
        );
        assert!(command(home.path(), "rust-analyzer").is_none());
    }
}
