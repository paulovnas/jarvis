use crate::mcp::{error, McpError};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

fn native_path(path: &Path) -> PathBuf {
    PathBuf::from(crate::library::strip_verbatim(&path.to_string_lossy()).as_ref())
}

fn find_executable(path: &Path) -> Option<PathBuf> {
    if path.extension().is_some() {
        return path.is_file().then(|| native_path(path));
    }
    ["exe", "com", "cmd", "bat"]
        .iter()
        .map(|extension| path.with_extension(extension))
        .find(|candidate| candidate.is_file())
        .map(|candidate| native_path(&candidate))
}

fn resolve(program: &str, search_path: &OsStr, directory: &Path) -> Option<PathBuf> {
    let program_path = Path::new(program);
    if program_path.is_absolute() || program.contains(['/', '\\']) {
        return find_executable(&directory.join(program_path));
    }
    std::env::split_paths(search_path)
        .find_map(|bin| find_executable(&directory.join(bin).join(program_path)))
}

pub(super) fn command(
    program: &str,
    search_path: &OsStr,
    directory: &Path,
) -> Result<tokio::process::Command, McpError> {
    let executable = resolve(program, search_path, directory).ok_or_else(|| {
        error("Executável do MCP não encontrado. Verifique a instalação e o PATH configurado.")
    })?;
    let bin = executable.parent().unwrap_or(directory);
    let name = executable
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    let is_batch = executable.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
    });
    // npm's Windows shims forward through cmd.exe. Invoke the same CLI with Node
    // directly so package names, paths and credentials remain literal argv entries.
    let cli = bin.join(format!("node_modules/npm/bin/{name}-cli.js"));
    let mut command = if is_batch && matches!(name.as_str(), "npm" | "npx") && cli.is_file() {
        let node = find_executable(&bin.join("node.exe"))
            .or_else(|| resolve("node.exe", search_path, directory))
            .ok_or_else(|| error("Node.js não encontrado para iniciar o MCP com npm/npx."))?;
        let mut command = tokio::process::Command::new(node);
        command.arg(native_path(&cli));
        command
    } else {
        // For explicit batch launchers, let Rust apply its Windows batch escaping.
        // Never join user arguments into a shell command line.
        tokio::process::Command::new(executable)
    };
    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn node() -> PathBuf {
        resolve(
            "node.exe",
            &super::super::configured_path(),
            &std::env::current_dir().unwrap(),
        )
        .expect("Node.js is required by the native MCP integration tests")
    }

    #[tokio::test]
    async fn npm_shims_preserve_literal_arguments_case_insensitive_path_and_native_cwd() {
        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("João Silva & tools");
        let project = temp.path().join("projeto com espaços");
        let cli_dir = bin.join("node_modules/npm/bin");
        std::fs::create_dir_all(&cli_dir).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        std::fs::copy(node(), bin.join("node.exe")).unwrap();
        let arguments = [
            "a b",
            "& echo injected",
            "x|y",
            "%PATH%",
            "!NAME!",
            "quote\"value",
            "C:\\João Silva\\file",
            "$(echo nope)",
        ];
        let environment = BTreeMap::from([
            ("Path".into(), bin.to_string_lossy().into_owned()),
            ("MCP_FIXTURE".into(), "preserved".into()),
        ]);
        for name in ["npm", "npx"] {
            std::fs::write(
                bin.join(format!("{name}.cmd")),
                "@echo SHIM_MUST_NOT_RUN\r\nexit /b 1",
            )
            .unwrap();
            std::fs::write(cli_dir.join(format!("{name}-cli.js")),
                "process.stdout.write(JSON.stringify({args:process.argv.slice(2),cwd:process.cwd(),env:process.env.MCP_FIXTURE}));").unwrap();
            let argv: Vec<String> = std::iter::once(name)
                .chain(arguments)
                .map(str::to_owned)
                .collect();
            let mut command =
                super::super::local_command(&argv, &environment, &project.canonicalize().unwrap())
                    .unwrap();
            let output = command.output().await.unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(result["args"], serde_json::json!(arguments));
            assert_eq!(
                Path::new(result["cwd"].as_str().unwrap())
                    .canonicalize()
                    .unwrap(),
                project.canonicalize().unwrap()
            );
            assert!(!result["cwd"].as_str().unwrap().starts_with(r"\\?\"));
            assert_eq!(result["env"], "preserved");
        }
    }

    #[tokio::test]
    async fn absolute_and_relative_executables_work_without_search_path_or_shell() {
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("runtime with spaces.exe");
        std::fs::copy(node(), &binary).unwrap();
        let environment = BTreeMap::from([("pAtH".into(), String::new())]);
        for program in [
            binary.to_string_lossy().into_owned(),
            r".\runtime with spaces.exe".into(),
        ] {
            let argv = vec![
                program,
                "-e".into(),
                "process.stdout.write('native-ready')".into(),
            ];
            let output = super::super::local_command(&argv, &environment, temp.path())
                .unwrap()
                .output()
                .await
                .unwrap();
            assert!(output.status.success());
            assert_eq!(output.stdout, b"native-ready");
        }
        assert!(
            super::super::local_command(&["node.exe".into()], &environment, temp.path()).is_err()
        );
        assert!(super::super::local_command(&[], &environment, temp.path()).is_err());
        assert!(super::super::local_command(
            &["node.exe".into()],
            &environment,
            &temp.path().join("missing")
        )
        .is_err());
    }
}
