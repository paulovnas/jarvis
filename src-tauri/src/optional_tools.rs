//! Optional developer tools surfaced during onboarding.
//!
//! Installation commands are selected from a closed enum. User-controlled text is
//! never passed to a shell or interpreted as command arguments.
use serde::{Deserialize, Serialize};
use std::process::Stdio;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OptionalToolId {
    Git,
    Gh,
}

impl OptionalToolId {
    const ALL: [Self; 2] = [Self::Git, Self::Gh];

    fn program(self) -> &'static str {
        match self {
            Self::Git => "git",
            Self::Gh => "gh",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Git => "Git",
            Self::Gh => "GitHub CLI",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Git => "Versiona as alterações e permite trabalhar com repositórios Git.",
            Self::Gh => "Habilita pull requests e merges assistidos pelo Jarvis no GitHub.",
        }
    }

    fn package(self, manager: PackageManager) -> &'static str {
        match (manager, self) {
            (PackageManager::Homebrew, Self::Git) => "git",
            (PackageManager::Homebrew, Self::Gh) => "gh",
            (PackageManager::Winget, Self::Git) => "Git.Git",
            (PackageManager::Winget, Self::Gh) => "GitHub.cli",
        }
    }

    fn help_url(self, platform: Platform) -> &'static str {
        match self {
            Self::Git => match platform {
                Platform::Macos => "https://git-scm.com/download/mac",
                Platform::Windows => "https://git-scm.com/download/win",
                Platform::Linux => "https://git-scm.com/download/linux",
                Platform::Other => "https://git-scm.com/downloads",
            },
            Self::Gh => "https://cli.github.com/",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Platform {
    Macos,
    Windows,
    Linux,
    Other,
}

impl Platform {
    fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Other
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Macos => "macos",
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Other => "other",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Macos => "macOS",
            Self::Windows => "Windows",
            Self::Linux => "Linux",
            Self::Other => "Sistema atual",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PackageManager {
    Homebrew,
    Winget,
}

impl PackageManager {
    fn program(self) -> &'static str {
        match self {
            Self::Homebrew => "brew",
            Self::Winget => "winget",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Homebrew => "Homebrew",
            Self::Winget => "WinGet",
        }
    }

    fn arguments(self, tool: OptionalToolId) -> Vec<&'static str> {
        match self {
            Self::Homebrew => vec!["install", tool.package(self)],
            Self::Winget => vec![
                "install",
                "--id",
                tool.package(self),
                "--exact",
                "--source",
                "winget",
                "--accept-package-agreements",
                "--accept-source-agreements",
                "--silent",
                "--disable-interactivity",
            ],
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OptionalTool {
    id: OptionalToolId,
    name: &'static str,
    description: &'static str,
    installed: bool,
    version: Option<String>,
    automatic_install: bool,
    install_with: Option<&'static str>,
    help_url: &'static str,
    checks: Vec<OptionalToolCheck>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OptionalToolCheck {
    id: &'static str,
    label: &'static str,
    ready: bool,
    message: &'static str,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OptionalToolsSnapshot {
    platform: &'static str,
    platform_label: &'static str,
    tools: Vec<OptionalTool>,
}

fn command(program: &str) -> std::process::Command {
    let mut command = crate::background::command(program);
    command
        .env("PATH", crate::mcp::executable::configured_path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn version(tool: OptionalToolId) -> Option<String> {
    let output = command(tool.program()).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn command_available(program: &str) -> bool {
    command(program)
        .arg("--version")
        .status()
        .is_ok_and(|status| status.success())
}

fn package_manager(
    platform: Platform,
    has_homebrew: bool,
    has_winget: bool,
) -> Option<PackageManager> {
    match platform {
        Platform::Macos if has_homebrew => Some(PackageManager::Homebrew),
        Platform::Windows if has_winget => Some(PackageManager::Winget),
        _ => None,
    }
}

fn current_package_manager(platform: Platform) -> Option<PackageManager> {
    // This is intentionally evaluated for every snapshot. A user can install
    // Homebrew or WinGet while onboarding is open and "Verificar novamente"
    // must make automatic installation available immediately.
    match platform {
        Platform::Macos => package_manager(platform, command_available("brew"), false),
        Platform::Windows => package_manager(platform, false, command_available("winget")),
        Platform::Linux | Platform::Other => package_manager(platform, false, false),
    }
}

fn configured_git_identity() -> bool {
    ["user.name", "user.email"].into_iter().all(|key| {
        command("git")
            .args(["config", "--global", "--get", key])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .is_some_and(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .chars()
                    .any(|character| !character.is_control())
            })
    })
}

fn authenticated_github_cli() -> bool {
    command("gh")
        .args(["auth", "status"])
        .env("GH_PROMPT_DISABLED", "1")
        .status()
        .is_ok_and(|status| status.success())
}

fn checks(id: OptionalToolId, installed: bool) -> Vec<OptionalToolCheck> {
    if !installed {
        return Vec::new();
    }
    match id {
        OptionalToolId::Git => {
            let ready = configured_git_identity();
            vec![OptionalToolCheck {
                id: "identity",
                label: "Identidade Git",
                ready,
                message: if ready {
                    "Nome e e-mail configurados para commits."
                } else {
                    "Configure nome e e-mail antes de publicar commits."
                },
            }]
        }
        OptionalToolId::Gh => {
            let ready = authenticated_github_cli();
            vec![OptionalToolCheck {
                id: "authentication",
                label: "GitHub conectado",
                ready,
                message: if ready {
                    "Autenticação do GitHub CLI confirmada."
                } else {
                    "Entre no GitHub CLI antes de criar pull requests ou merges."
                },
            }]
        }
    }
}

fn snapshot() -> OptionalToolsSnapshot {
    let platform = Platform::current();
    let manager = current_package_manager(platform);
    let tools = OptionalToolId::ALL
        .into_iter()
        .map(|id| {
            let version = version(id);
            let installed = version.is_some();
            OptionalTool {
                id,
                name: id.name(),
                description: id.description(),
                installed,
                version,
                automatic_install: manager.is_some(),
                install_with: manager.map(PackageManager::label),
                help_url: id.help_url(platform),
                checks: checks(id, installed),
            }
        })
        .collect();
    OptionalToolsSnapshot {
        platform: platform.id(),
        platform_label: platform.label(),
        tools,
    }
}

fn output_detail(output: &std::process::Output) -> String {
    let bytes = if output.stderr.is_empty() {
        &output.stdout
    } else {
        &output.stderr
    };
    String::from_utf8_lossy(bytes)
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .chars()
        .filter(|character| !character.is_control())
        .take(500)
        .collect()
}

fn install(tool: OptionalToolId) -> Result<OptionalToolsSnapshot, String> {
    if version(tool).is_some() {
        return Ok(snapshot());
    }
    let platform = Platform::current();
    let manager = current_package_manager(platform).ok_or_else(|| {
        format!(
            "A instalação automática de {} não está disponível neste sistema. Abra as instruções oficiais.",
            tool.name()
        )
    })?;
    let mut installer = command(manager.program());
    installer.args(manager.arguments(tool));
    if manager == PackageManager::Homebrew {
        installer.env("HOMEBREW_NO_AUTO_UPDATE", "1");
    }
    let output = installer.output().map_err(|_| {
        format!(
            "Não foi possível iniciar a instalação de {} com {}.",
            tool.name(),
            manager.label()
        )
    })?;
    if !output.status.success() {
        let detail = output_detail(&output);
        let suffix = if detail.is_empty() {
            String::new()
        } else {
            format!(" {detail}")
        };
        return Err(format!(
            "Não foi possível instalar {} com {}.{suffix}",
            tool.name(),
            manager.label()
        ));
    }
    if version(tool).is_none() {
        return Err(format!(
            "A instalação de {} terminou, mas o executável ainda não foi encontrado. Reinicie o Jarvis e verifique novamente.",
            tool.name()
        ));
    }
    Ok(snapshot())
}

#[tauri::command]
pub async fn get_optional_tools_status() -> Result<OptionalToolsSnapshot, String> {
    tauri::async_runtime::spawn_blocking(snapshot)
        .await
        .map_err(|_| "Não foi possível verificar as ferramentas opcionais.".to_string())
}

#[tauri::command]
pub async fn install_optional_tool(id: OptionalToolId) -> Result<OptionalToolsSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || install(id))
        .await
        .map_err(|_| "A instalação da ferramenta opcional foi interrompida.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installer_selection_is_platform_specific_and_optional() {
        assert_eq!(
            package_manager(Platform::Macos, true, true),
            Some(PackageManager::Homebrew)
        );
        assert_eq!(
            package_manager(Platform::Windows, true, true),
            Some(PackageManager::Winget)
        );
        assert_eq!(package_manager(Platform::Linux, true, true), None);
        assert_eq!(package_manager(Platform::Macos, false, false), None);
    }

    #[test]
    fn install_arguments_are_closed_and_non_interactive() {
        assert_eq!(
            PackageManager::Homebrew.arguments(OptionalToolId::Gh),
            ["install", "gh"]
        );
        let windows = PackageManager::Winget.arguments(OptionalToolId::Git);
        assert!(windows.windows(2).any(|pair| pair == ["--id", "Git.Git"]));
        assert!(windows.contains(&"--disable-interactivity"));
        assert!(windows.contains(&"--accept-package-agreements"));
    }

    #[test]
    fn manual_links_follow_the_operating_system() {
        assert!(OptionalToolId::Git
            .help_url(Platform::Windows)
            .ends_with("/win"));
        assert!(OptionalToolId::Git
            .help_url(Platform::Macos)
            .ends_with("/mac"));
        assert_eq!(
            OptionalToolId::Gh.help_url(Platform::Linux),
            "https://cli.github.com/"
        );
    }
}
