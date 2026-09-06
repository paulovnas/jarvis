use std::{collections::HashSet, ffi::{OsStr, OsString}, path::{Path, PathBuf}};

// GUI launches inherit a minimal PATH. Discover installed runtime bins without
// interpreting server commands or sourcing arbitrary shell startup output.
pub(super) fn search_path(inherited: &OsStr, home: &Path, prefixes: &[&Path]) -> OsString {
    let mut paths: Vec<PathBuf> = std::env::split_paths(inherited).filter(|path| path.is_absolute()).collect();
    for prefix in prefixes {
        paths.push(prefix.join("bin"));
        let mut node: Vec<_> = std::fs::read_dir(prefix.join("opt")).into_iter().flatten().flatten()
            .filter(|entry| entry.file_name() == "node" || entry.file_name().to_string_lossy().starts_with("node@"))
            .map(|entry| entry.path().join("bin")).filter(|bin| bin.join("node").is_file()).collect();
        node.sort_by(|a, b| b.cmp(a));
        paths.extend(node);
    }
    paths.extend([".local/bin", ".bun/bin", ".volta/bin", ".asdf/shims", ".local/share/mise/shims"].map(|part| home.join(part)));
    let mut nvm: Vec<_> = std::fs::read_dir(home.join(".nvm/versions/node")).into_iter().flatten().flatten()
        .map(|entry| entry.path().join("bin")).filter(|bin| bin.join("node").is_file()).collect();
    nvm.sort_by_key(|bin| std::cmp::Reverse(bin.parent().and_then(Path::file_name).unwrap_or_default().to_string_lossy().trim_start_matches('v').split('.').map(|part| part.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>()));
    paths.extend(nvm);
    paths.extend(["/usr/bin", "/bin", "/usr/sbin", "/sbin"].map(PathBuf::from));
    let mut seen = HashSet::new();
    paths.retain(|path| path.is_dir() && seen.insert(path.clone()));
    std::env::join_paths(paths).unwrap_or_else(|_| inherited.to_owned())
}

pub(super) fn configure(command: &mut tokio::process::Command, explicit_path: bool) {
    if explicit_path { return; }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    command.env("PATH", search_path(&std::env::var_os("PATH").unwrap_or_default(), &home, &[Path::new("/opt/homebrew"), Path::new("/usr/local")]));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[tokio::test]
    async fn gui_path_resolves_npx_and_its_node_shebang_without_shell_evaluation() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let prefix = temp.path().join("brew");
        let bin = prefix.join("opt/node@22/bin");
        std::fs::create_dir_all(&bin).unwrap();
        for (name, contents) in [("node", "#!/bin/sh\nprintf 'runtime-ready'"), ("npx", "#!/usr/bin/env node\n")] {
            let file = bin.join(name);
            std::fs::write(&file, contents).unwrap();
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = search_path(OsStr::new("/usr/bin:/bin"), temp.path(), &[&prefix]);
        let output = tokio::process::Command::new("npx").env_clear().env("PATH", path).output().await.unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"runtime-ready");
        let mut command = tokio::process::Command::new("npx");
        command.env("PATH", "/explicit/runtime");
        configure(&mut command, true);
        assert!(command.as_std().get_envs().any(|(key, value)| key == "PATH" && value == Some(OsStr::new("/explicit/runtime"))));
    }
}
