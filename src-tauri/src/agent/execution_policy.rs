//! Deterministic admission policy for local execution surfaces.
//!
//! The model supplies text, but policy operates on a parsed command plan,
//! normalized roots, and declared tool capabilities. Unknown or dynamic shell
//! syntax never inherits the classification of a neighboring safe command.

use super::{
    tool_contract::{Capabilities, Effect},
    AgentError, ToolCall,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(super) enum ExecutionDecision {
    Allow,
    Ask,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum NetworkPolicy {
    Deny,
    Ask,
    Allow,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExecutionEffects {
    pub reads_filesystem: bool,
    pub writes_filesystem: bool,
    pub uses_network: bool,
    pub controls_processes: bool,
    pub destructive: bool,
    pub dynamic: bool,
    pub unknown: bool,
}

impl ExecutionEffects {
    fn merge(&mut self, other: Self) {
        self.reads_filesystem |= other.reads_filesystem;
        self.writes_filesystem |= other.writes_filesystem;
        self.uses_network |= other.uses_network;
        self.controls_processes |= other.controls_processes;
        self.destructive |= other.destructive;
        self.dynamic |= other.dynamic;
        self.unknown |= other.unknown;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(super) struct CommandInvocation {
    pub argv: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(super) struct Redirection {
    pub target: String,
    pub write: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(super) struct CommandPlan {
    pub invocations: Vec<CommandInvocation>,
    pub redirections: Vec<Redirection>,
    pub dynamic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ExecutionScope {
    pub project_root: PathBuf,
    pub working_directory: PathBuf,
    pub readable_roots: Vec<PathBuf>,
    pub writable_roots: Vec<PathBuf>,
    pub network: NetworkPolicy,
}

impl ExecutionScope {
    pub(super) fn project(project_root: &Path, working_directory: &Path) -> Self {
        let project_root = lexical_absolute(project_root, project_root);
        let working_directory = lexical_absolute(&project_root, working_directory);
        Self {
            readable_roots: vec![project_root.clone()],
            writable_roots: vec![project_root.clone()],
            project_root,
            working_directory,
            network: NetworkPolicy::Ask,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PolicyOutcome {
    pub decision: ExecutionDecision,
    pub code: String,
    pub reason: String,
    pub effects: ExecutionEffects,
    pub command: Option<CommandPlan>,
    pub read_paths: Vec<PathBuf>,
    pub write_paths: Vec<PathBuf>,
    #[serde(default)]
    pub native_working_directory: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ExecutionOperation {
    Command(CommandPlan),
    Filesystem {
        read_paths: Vec<PathBuf>,
        write_paths: Vec<PathBuf>,
    },
    #[cfg_attr(not(test), allow(dead_code))]
    Network,
    Declared {
        read_paths: Vec<PathBuf>,
        write_paths: Vec<PathBuf>,
        network: bool,
        persistent_process: bool,
        destructive: bool,
    },
    ProcessControl,
    PersistentProcess(CommandPlan),
}

#[derive(Debug, Clone)]
pub(super) struct PolicyRequest<'a> {
    pub tool_name: &'a str,
    pub capabilities: Capabilities,
    pub scope: &'a ExecutionScope,
    pub operation: ExecutionOperation,
}

#[derive(Debug, Clone)]
pub(super) struct ToolPolicy {
    pub outcome: PolicyOutcome,
    pub project_root: PathBuf,
    pub working_directory: PathBuf,
}

pub(super) fn inspect_tool(
    project_root: &Path,
    tool: &ToolCall,
    capabilities: Capabilities,
) -> Result<Option<ToolPolicy>, AgentError> {
    let workdir = tool.args["workdir"].as_str().unwrap_or(".");
    let working_directory = if tool.name == "bash" {
        super::tools::scoped(project_root, workdir, false)?
    } else {
        project_root.to_path_buf()
    };
    let scope = ExecutionScope::project(project_root, &working_directory);
    let operation = match tool.name.as_str() {
        "read" | "list" | "search" => ExecutionOperation::Filesystem {
            read_paths: vec![PathBuf::from(tool.args["path"].as_str().unwrap_or("."))],
            write_paths: vec![],
        },
        "write" | "edit" => ExecutionOperation::Filesystem {
            read_paths: (tool.name == "edit")
                .then(|| PathBuf::from(tool.args["path"].as_str().unwrap_or(".")))
                .into_iter()
                .collect(),
            write_paths: vec![PathBuf::from(tool.args["path"].as_str().unwrap_or("."))],
        },
        "apply_patch" => ExecutionOperation::Filesystem {
            read_paths: vec![],
            write_paths: super::patch::target_paths(&tool.args)?
                .into_iter()
                .map(PathBuf::from)
                .collect(),
        },
        "bash" => ExecutionOperation::Command(parse_or_dynamic(
            tool.args["command"].as_str().unwrap_or_default(),
        )),
        "terminal_start" | "process_start" => ExecutionOperation::PersistentProcess(
            parse_or_dynamic(tool.args["command"].as_str().unwrap_or("interactive-shell")),
        ),
        "terminal_close" => ExecutionOperation::ProcessControl,
        "jarvis_propose_publication" => publication_operation(&tool.args, project_root),
        _ => return Ok(None),
    };
    let mut outcome = evaluate(PolicyRequest {
        tool_name: &tool.name,
        capabilities,
        scope: &scope,
        operation,
    });
    let native_requested = tool.args["sandboxPermissions"] == "require_escalated";
    let external_command = outcome.command.is_some()
        && matches!(
            outcome.code.as_str(),
            "read_scope_escape" | "write_scope_escape"
        );
    if native_requested || external_command {
        if native_requested
            && !tool.args["justification"]
                .as_str()
                .is_some_and(|reason| !reason.trim().is_empty() && reason.len() <= 1_000)
        {
            return Err(AgentError::new("permission_justification_required", "Explique em justification o acesso adicional necessário para executar fora do isolamento."));
        }
        if outcome.command.is_some()
            && (outcome.decision != ExecutionDecision::Deny || external_command)
        {
            outcome.decision = ExecutionDecision::Ask;
            outcome.code = "native_execution_approval_required".into();
            outcome.reason = format!(
                "O comando precisa executar fora do isolamento de arquivos e rede do Jarvis. {}",
                tool.args["justification"].as_str().unwrap_or("A ação acessa caminhos externos ao projeto e seguirá o modo de aprovação ativo.")
            );
            outcome.native_working_directory = Some(working_directory.clone());
        }
    }
    Ok(Some(ToolPolicy {
        outcome,
        project_root: project_root.to_path_buf(),
        working_directory,
    }))
}

fn parse_or_dynamic(command: &str) -> CommandPlan {
    parse_command(command).unwrap_or_else(|_| CommandPlan {
        invocations: vec![CommandInvocation {
            argv: vec![command.to_owned()],
        }],
        redirections: vec![],
        dynamic: true,
    })
}

fn publication_operation(arguments: &serde_json::Value, root: &Path) -> ExecutionOperation {
    let mut write_paths = Vec::new();
    let mut network = false;
    let mut destructive = false;
    if let Some(repositories) = arguments["repositories"].as_array() {
        for repository in repositories {
            let path = repository["path"].as_str().unwrap_or(".");
            write_paths.push(root.join(path));
            network |= repository["sync"]
                .as_str()
                .is_some_and(|sync| sync != "none")
                || repository["push"] != "none"
                || !repository["pullRequest"].is_null();
            destructive |= !repository["reset"].is_null();
            destructive |= repository["sync"] == "rebase";
        }
    }
    ExecutionOperation::Declared {
        read_paths: write_paths.clone(),
        write_paths,
        network,
        persistent_process: false,
        destructive,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Analysis {
    effects: ExecutionEffects,
    reads: Vec<PathBuf>,
    writes: Vec<PathBuf>,
    privileged: bool,
}

impl Analysis {
    fn new() -> Self {
        Self {
            effects: ExecutionEffects::default(),
            reads: Vec::new(),
            writes: Vec::new(),
            privileged: false,
        }
    }

    fn merge(&mut self, other: Self) {
        self.effects.merge(other.effects);
        self.reads.extend(other.reads);
        self.writes.extend(other.writes);
        self.privileged |= other.privileged;
    }
}

pub(super) fn evaluate(request: PolicyRequest<'_>) -> PolicyOutcome {
    let command = match &request.operation {
        ExecutionOperation::Command(plan) | ExecutionOperation::PersistentProcess(plan) => {
            Some(plan.clone())
        }
        ExecutionOperation::Filesystem { .. }
        | ExecutionOperation::Network
        | ExecutionOperation::Declared { .. }
        | ExecutionOperation::ProcessControl => None,
    };
    let mut analysis = analyze_operation(&request.operation, request.scope);
    normalize_paths(&mut analysis.reads);
    normalize_paths(&mut analysis.writes);

    if analysis.privileged {
        return outcome(
            ExecutionDecision::Deny,
            "privilege_escalation_denied",
            "O comando tenta elevar privilégios fora do escopo do Jarvis.",
            analysis,
            command,
        );
    }
    // Hard policy restrictions remain final even when a command also accesses
    // an external path, which can otherwise request informed approval.
    if capability_conflicts(request.capabilities.effect, &analysis.effects) {
        return outcome(
            ExecutionDecision::Deny,
            "tool_capability_mismatch",
            &format!(
                "A ferramenta '{}' não declarou capacidade para os efeitos detectados.",
                request.tool_name
            ),
            analysis,
            command,
        );
    }
    if analysis.effects.uses_network && request.scope.network == NetworkPolicy::Deny {
        return outcome(
            ExecutionDecision::Deny,
            "network_scope_denied",
            "A rede não está disponível para este escopo de execução.",
            analysis,
            command,
        );
    }
    if !paths_within(&analysis.reads, &request.scope.readable_roots) {
        return outcome(
            ExecutionDecision::Deny,
            "read_scope_escape",
            "O comando tenta ler um caminho fora dos roots autorizados.",
            analysis,
            command,
        );
    }
    if !paths_within(&analysis.writes, &request.scope.writable_roots) {
        return outcome(
            ExecutionDecision::Deny,
            "write_scope_escape",
            "O comando tenta alterar um caminho fora dos roots autorizados.",
            analysis,
            command,
        );
    }
    if analysis.effects.uses_network && request.scope.network == NetworkPolicy::Ask {
        return outcome(
            ExecutionDecision::Ask,
            "network_approval_required",
            "O comando pode usar rede, incluindo serviços locais, testes e servidores de desenvolvimento.",
            analysis,
            command,
        );
    }
    if analysis.effects.dynamic {
        return outcome(
            ExecutionDecision::Ask,
            "dynamic_command_approval_required",
            "O comando contém expansão ou controle dinâmico que precisa de revisão.",
            analysis,
            command,
        );
    }
    if analysis.effects.destructive {
        return outcome(
            ExecutionDecision::Ask,
            "destructive_command_approval_required",
            "O comando pode remover ou sobrescrever dados.",
            analysis,
            command,
        );
    }
    if analysis.effects.controls_processes {
        return outcome(
            ExecutionDecision::Ask,
            "process_approval_required",
            "O comando inicia, encerra ou controla um processo persistente.",
            analysis,
            command,
        );
    }
    if analysis.effects.writes_filesystem {
        return outcome(
            ExecutionDecision::Ask,
            "write_approval_required",
            "O comando pode alterar arquivos dentro dos roots autorizados.",
            analysis,
            command,
        );
    }
    if analysis.effects.unknown {
        return outcome(
            ExecutionDecision::Ask,
            "unknown_command_approval_required",
            "O efeito completo do comando não pôde ser determinado.",
            analysis,
            command,
        );
    }
    outcome(
        ExecutionDecision::Allow,
        "read_only_within_scope",
        "A ação é somente leitura e permanece nos roots autorizados.",
        analysis,
        command,
    )
}

fn outcome(
    decision: ExecutionDecision,
    code: &str,
    reason: &str,
    analysis: Analysis,
    command: Option<CommandPlan>,
) -> PolicyOutcome {
    PolicyOutcome {
        decision,
        code: code.to_owned(),
        reason: reason.to_owned(),
        effects: analysis.effects,
        command,
        read_paths: analysis.reads,
        write_paths: analysis.writes,
        native_working_directory: None,
    }
}

fn capability_conflicts(effect: Effect, effects: &ExecutionEffects) -> bool {
    effect == Effect::ReadOnly
        && (effects.writes_filesystem || effects.uses_network || effects.controls_processes)
}

fn analyze_operation(operation: &ExecutionOperation, scope: &ExecutionScope) -> Analysis {
    match operation {
        ExecutionOperation::Command(plan) => analyze_command(plan, scope, false),
        ExecutionOperation::PersistentProcess(plan) => analyze_command(plan, scope, true),
        ExecutionOperation::Filesystem {
            read_paths,
            write_paths,
        } => {
            let mut analysis = Analysis::new();
            analysis.effects.reads_filesystem = !read_paths.is_empty();
            analysis.effects.writes_filesystem = !write_paths.is_empty();
            analysis.reads = read_paths
                .iter()
                .map(|path| lexical_absolute(&scope.working_directory, path))
                .collect();
            analysis.writes = write_paths
                .iter()
                .map(|path| lexical_absolute(&scope.working_directory, path))
                .collect();
            analysis
        }
        ExecutionOperation::Network => {
            let mut analysis = Analysis::new();
            analysis.effects.uses_network = true;
            analysis
        }
        ExecutionOperation::Declared {
            read_paths,
            write_paths,
            network,
            persistent_process,
            destructive,
        } => {
            let mut analysis = Analysis::new();
            analysis.effects.reads_filesystem = !read_paths.is_empty();
            analysis.effects.writes_filesystem = !write_paths.is_empty();
            analysis.effects.uses_network = *network;
            analysis.effects.controls_processes = *persistent_process;
            analysis.effects.destructive = *destructive;
            analysis.reads = read_paths
                .iter()
                .map(|path| lexical_absolute(&scope.working_directory, path))
                .collect();
            analysis.writes = write_paths
                .iter()
                .map(|path| lexical_absolute(&scope.working_directory, path))
                .collect();
            analysis
        }
        ExecutionOperation::ProcessControl => {
            let mut analysis = Analysis::new();
            analysis.effects.controls_processes = true;
            analysis
        }
    }
}

fn analyze_command(plan: &CommandPlan, scope: &ExecutionScope, persistent: bool) -> Analysis {
    let mut analysis = Analysis::new();
    analysis.effects.dynamic = plan.dynamic;
    analysis.effects.controls_processes = persistent;
    for invocation in &plan.invocations {
        analysis.merge(analyze_invocation(invocation, scope));
    }
    // A script, wrapper or interactive process can open sockets without naming
    // a network utility. Include that capability in admission and the grant;
    // approving an opaque command must not leave its children offline.
    analysis.effects.uses_network |=
        analysis.effects.dynamic || analysis.effects.unknown || persistent;
    for redirect in &plan.redirections {
        // Discarding output or reading EOF is standard I/O, not a project
        // mutation. Only the actual Unix null device gets this exception;
        // operations such as rm/mv still go through the normal path checks.
        #[cfg(unix)]
        if redirect.target == "/dev/null" {
            use std::os::unix::fs::FileTypeExt;
            if std::fs::symlink_metadata(&redirect.target)
                .is_ok_and(|metadata| metadata.file_type().is_char_device())
            {
                continue;
            }
        }
        let target = lexical_absolute(&scope.working_directory, Path::new(&redirect.target));
        if redirect.write {
            analysis.effects.writes_filesystem = true;
            analysis.writes.push(target);
        } else {
            analysis.effects.reads_filesystem = true;
            analysis.reads.push(target);
        }
    }
    analysis
}

fn analyze_invocation(invocation: &CommandInvocation, scope: &ExecutionScope) -> Analysis {
    let mut analysis = Analysis::new();
    let Some((program, args)) = program_and_args(&invocation.argv) else {
        analysis.effects.unknown = true;
        return analysis;
    };
    let program = executable_name(program);
    if matches!(program.as_str(), "sudo" | "su" | "doas" | "runas") {
        analysis.privileged = true;
        return analysis;
    }
    match program.as_str() {
        "pwd" | "whoami" | "uname" | "date" | "true" | "false" | "which" => {}
        "echo" | "printf" => {
            analysis.effects.dynamic |= args.iter().any(|arg| is_dynamic(arg));
        }
        "ls" | "cat" | "head" | "tail" | "wc" | "stat" | "file" | "du" => {
            analysis.effects.reads_filesystem = true;
            analysis.reads.extend(
                path_arguments(args)
                    .map(|path| lexical_absolute(&scope.working_directory, Path::new(path))),
            );
        }
        "rg" | "grep" => {
            analysis.effects.reads_filesystem = true;
            if args
                .iter()
                .any(|arg| matches!(arg.as_str(), "--pre" | "--pre-glob"))
            {
                analysis.effects.dynamic = true;
            }
            analysis.reads.extend(
                search_paths(args)
                    .map(|path| lexical_absolute(&scope.working_directory, Path::new(path))),
            );
        }
        "find" => {
            analysis.effects.reads_filesystem = true;
            analysis.effects.dynamic |= args.iter().any(|arg| {
                matches!(
                    arg.as_str(),
                    "-exec" | "-execdir" | "-delete" | "-ok" | "-okdir"
                )
            });
            analysis.reads.extend(
                args.iter()
                    .take_while(|arg| !arg.starts_with('-'))
                    .map(|path| lexical_absolute(&scope.working_directory, Path::new(path))),
            );
        }
        "sed" => {
            analysis.effects.reads_filesystem = true;
            let in_place = args
                .iter()
                .any(|arg| arg == "-i" || arg.starts_with("--in-place"));
            analysis.effects.writes_filesystem = in_place;
            for path in args.iter().rev().take_while(|arg| !arg.starts_with('-')) {
                let path = lexical_absolute(&scope.working_directory, Path::new(path));
                analysis.reads.push(path.clone());
                if in_place {
                    analysis.writes.push(path);
                }
            }
        }
        "mkdir" | "touch" | "chmod" | "chown" | "truncate" => {
            analysis.effects.writes_filesystem = true;
            analysis.writes.extend(
                path_arguments(args)
                    .map(|path| lexical_absolute(&scope.working_directory, Path::new(path))),
            );
        }
        "rm" => {
            analysis.effects.writes_filesystem = true;
            analysis.effects.destructive = true;
            analysis.writes.extend(
                path_arguments(args)
                    .map(|path| lexical_absolute(&scope.working_directory, Path::new(path))),
            );
        }
        "cp" | "install" => {
            analysis.effects.reads_filesystem = true;
            analysis.effects.writes_filesystem = true;
            let paths = path_arguments(args).collect::<Vec<_>>();
            if let Some((destination, sources)) = paths.split_last() {
                analysis.writes.push(lexical_absolute(
                    &scope.working_directory,
                    Path::new(destination),
                ));
                analysis.reads.extend(
                    sources
                        .iter()
                        .map(|path| lexical_absolute(&scope.working_directory, Path::new(path))),
                );
            }
        }
        "mv" => {
            analysis.effects.reads_filesystem = true;
            analysis.effects.writes_filesystem = true;
            analysis.effects.destructive = true;
            for path in path_arguments(args) {
                let path = lexical_absolute(&scope.working_directory, Path::new(path));
                analysis.reads.push(path.clone());
                analysis.writes.push(path);
            }
        }
        "git" => analyze_git(args, scope, &mut analysis),
        "curl" | "wget" | "ssh" | "scp" | "sftp" | "ftp" | "nc" | "ncat" => {
            analysis.effects.uses_network = true;
            analysis.effects.writes_filesystem =
                matches!(program.as_str(), "curl" | "wget" | "scp" | "sftp");
            if analysis.effects.writes_filesystem {
                analysis.writes.push(scope.working_directory.clone());
            }
        }
        "gh" => {
            analysis.effects.uses_network = true;
            analysis.effects.writes_filesystem = true;
            analysis.writes.push(scope.working_directory.clone());
        }
        "kill" | "pkill" | "killall" => {
            analysis.effects.controls_processes = true;
            analysis.effects.destructive = true;
        }
        "npm" | "pnpm" | "yarn" | "bun" | "cargo" | "go" | "python" | "python3" | "pip"
        | "pip3" | "ruby" | "php" | "node" | "deno" => {
            analysis.effects.writes_filesystem = true;
            analysis.effects.uses_network = package_command_uses_network(&program, args);
            analysis.writes.push(scope.working_directory.clone());
        }
        "bash" | "sh" | "zsh" | "fish" | "powershell" | "pwsh" | "cmd" | "xargs" => {
            analysis.effects.dynamic = true;
            analysis.effects.unknown = true;
        }
        _ => analysis.effects.unknown = true,
    }
    analysis
}

fn analyze_git(args: &[String], scope: &ExecutionScope, analysis: &mut Analysis) {
    let mut directory = scope.working_directory.clone();
    let mut additional_roots = Vec::new();
    let mut index = 0;
    while let Some(argument) = args.get(index).filter(|arg| arg.starts_with('-')) {
        match argument.as_str() {
            "-C" | "--git-dir" | "--work-tree" => {
                let Some(value) = args.get(index + 1) else {
                    analysis.effects.unknown = true;
                    return;
                };
                let target = lexical_absolute(&directory, Path::new(value));
                if argument == "-C" {
                    directory = target;
                } else {
                    additional_roots.push(target);
                }
                index += 2;
            }
            "-c" | "--config-env" => {
                analysis.effects.dynamic = true;
                index += 2;
            }
            _ => {
                if let Some(value) = argument
                    .strip_prefix("--git-dir=")
                    .or_else(|| argument.strip_prefix("--work-tree="))
                {
                    additional_roots.push(lexical_absolute(&directory, Path::new(value)));
                } else if argument.starts_with("--config-env=") {
                    analysis.effects.dynamic = true;
                }
                index += 1;
            }
        }
    }
    analysis.effects.reads_filesystem = true;
    analysis.reads.push(directory.clone());
    let subcommand = args.get(index).map(String::as_str);
    match subcommand {
        Some("status" | "diff" | "log" | "show" | "rev-parse" | "ls-files" | "grep") => {}
        Some("branch")
            if !args.iter().any(|arg| {
                matches!(
                    arg.as_str(),
                    "-d" | "-D" | "-m" | "-M" | "--set-upstream-to"
                )
            }) => {}
        Some("remote")
            if args
                .iter()
                .any(|arg| matches!(arg.as_str(), "-v" | "get-url")) => {}
        Some("push" | "pull" | "fetch" | "clone" | "ls-remote" | "submodule") => {
            analysis.effects.uses_network = true;
            analysis.effects.writes_filesystem = !matches!(subcommand, Some("push" | "ls-remote"));
            if analysis.effects.writes_filesystem {
                analysis.writes.push(directory.clone());
            }
        }
        Some("clean") => {
            analysis.effects.writes_filesystem = true;
            analysis.effects.destructive = true;
            analysis.writes.push(directory.clone());
        }
        Some(_) => {
            analysis.effects.writes_filesystem = true;
            analysis.writes.push(directory);
        }
        None => analysis.effects.unknown = true,
    }
    analysis.reads.extend(additional_roots.iter().cloned());
    if analysis.effects.writes_filesystem {
        analysis.writes.extend(additional_roots);
    }
}

fn package_command_uses_network(program: &str, args: &[String]) -> bool {
    if matches!(args, [flag] if matches!(flag.as_str(), "--version" | "-v" | "-V" | "--help" | "-h"))
    {
        return false;
    }
    // Tests/builds can reach local databases, bind worker ports, fetch build
    // inputs or run arbitrary project code. Only known metadata queries can
    // exclude network access; a short installer allowlist is not sufficient.
    !matches!(args, [action] if matches!((program, action.as_str()),
        ("cargo" | "go", "version" | "help") | ("pip" | "pip3", "help")))
}

fn program_and_args(argv: &[String]) -> Option<(&str, &[String])> {
    let index = argv
        .iter()
        .position(|arg| !is_environment_assignment(arg))?;
    Some((&argv[index], &argv[index + 1..]))
}

fn executable_name(program: &str) -> String {
    Path::new(program)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(program)
        .to_ascii_lowercase()
}

fn is_environment_assignment(value: &str) -> bool {
    value.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && name
                .chars()
                .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
    })
}

fn is_dynamic(value: &str) -> bool {
    value.contains("$(") || value.contains('`') || value.contains("${")
}

fn path_arguments(args: &[String]) -> impl Iterator<Item = &str> {
    args.iter()
        .filter(|arg| !arg.starts_with('-'))
        .map(String::as_str)
}

fn search_paths(args: &[String]) -> impl Iterator<Item = &str> {
    let positional = path_arguments(args).collect::<Vec<_>>();
    positional.into_iter().skip(1)
}

fn paths_within(paths: &[PathBuf], roots: &[PathBuf]) -> bool {
    paths
        .iter()
        .all(|path| roots.iter().any(|root| path.starts_with(root)))
}

fn normalize_paths(paths: &mut Vec<PathBuf>) {
    let mut unique = BTreeSet::new();
    paths.retain(|path| unique.insert(path.clone()));
}

fn lexical_absolute(base: &Path, path: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

pub(super) fn parse_command(command: &str) -> Result<CommandPlan, &'static str> {
    let tokens = lex(command)?;
    let mut invocations = Vec::new();
    let mut redirections = Vec::new();
    let mut current = Vec::new();
    let mut dynamic = false;
    let mut index = 0;
    while index < tokens.len() {
        match tokens[index].as_str() {
            ";" | "&&" | "||" | "|" => {
                if current.is_empty() {
                    return Err("operador sem comando");
                }
                invocations.push(CommandInvocation {
                    argv: std::mem::take(&mut current),
                });
            }
            ">" | ">>" | "1>" | "1>>" | "2>" | "2>>" | "<" => {
                let Some(target) = tokens.get(index + 1) else {
                    return Err("redirecionamento sem destino");
                };
                redirections.push(Redirection {
                    target: target.clone(),
                    write: tokens[index] != "<",
                });
                index += 1;
            }
            token => {
                dynamic |= is_dynamic(token) || matches!(token, "(" | ")" | "{" | "}");
                current.push(token.to_owned());
            }
        }
        index += 1;
    }
    if !current.is_empty() {
        invocations.push(CommandInvocation { argv: current });
    }
    if invocations.is_empty() {
        return Err("comando vazio");
    }
    Ok(CommandPlan {
        invocations,
        redirections,
        dynamic,
    })
}

fn lex(command: &str) -> Result<Vec<String>, &'static str> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Quote {
        None,
        Single,
        Double,
    }
    let chars = command.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = Quote::None;
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        match quote {
            Quote::Single => {
                if ch == '\'' {
                    quote = Quote::None;
                } else {
                    current.push(ch);
                }
            }
            Quote::Double => {
                if ch == '"' {
                    quote = Quote::None;
                } else if ch == '\\' {
                    index += 1;
                    current.push(*chars.get(index).ok_or("escape incompleto")?);
                } else {
                    current.push(ch);
                }
            }
            Quote::None => match ch {
                '\'' => quote = Quote::Single,
                '"' => quote = Quote::Double,
                '\\' => {
                    index += 1;
                    current.push(*chars.get(index).ok_or("escape incompleto")?);
                }
                ch if ch.is_whitespace() => push_token(&mut tokens, &mut current),
                ';' | '|' | '&' | '<' | '>' => {
                    push_token(&mut tokens, &mut current);
                    let mut operator = ch.to_string();
                    if chars.get(index + 1) == Some(&ch) && matches!(ch, '|' | '&' | '>') {
                        index += 1;
                        operator.push(ch);
                    }
                    tokens.push(operator);
                }
                _ => current.push(ch),
            },
        }
        index += 1;
    }
    if quote != Quote::None {
        return Err("aspas não fechadas");
    }
    push_token(&mut tokens, &mut current);
    Ok(tokens)
}

fn push_token(tokens: &mut Vec<String>, current: &mut String) {
    if !current.is_empty() {
        tokens.push(std::mem::take(current));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capabilities(effect: Effect) -> Capabilities {
        Capabilities {
            effect,
            approval: super::super::tool_contract::ApprovalPolicy::AccordingToTurn,
            parallel_safe: false,
        }
    }

    fn tool(name: &str, args: serde_json::Value) -> ToolCall {
        ToolCall {
            id: "call".into(),
            name: name.into(),
            args,
            status: "pending".into(),
            output: String::new(),
            duration_ms: 0,
        }
    }

    fn decide(command: &str, network: NetworkPolicy) -> PolicyOutcome {
        let root = Path::new("/workspace/project");
        let mut scope = ExecutionScope::project(root, &root.join("backend"));
        scope.network = network;
        evaluate(PolicyRequest {
            tool_name: "bash",
            capabilities: capabilities(Effect::Mutating),
            scope: &scope,
            operation: ExecutionOperation::Command(parse_command(command).unwrap()),
        })
    }

    #[test]
    fn parser_preserves_quoted_argv_and_splits_compound_commands() {
        let plan = parse_command("rg 'two words' src && git status --short").unwrap();
        assert_eq!(plan.invocations.len(), 2);
        assert_eq!(plan.invocations[0].argv, ["rg", "two words", "src"]);
        assert_eq!(plan.invocations[1].argv, ["git", "status", "--short"]);
        assert!(!plan.dynamic);
    }

    #[test]
    fn read_only_command_inside_root_is_allowed() {
        let outcome = decide("rg todo src && git status --short", NetworkPolicy::Deny);
        assert_eq!(outcome.decision, ExecutionDecision::Allow);
        assert_eq!(outcome.code, "read_only_within_scope");
    }

    #[test]
    fn writes_inside_root_require_approval() {
        let outcome = decide("mkdir -p src/generated", NetworkPolicy::Deny);
        assert_eq!(outcome.decision, ExecutionDecision::Ask);
        assert_eq!(outcome.code, "write_approval_required");
        assert_eq!(
            outcome.write_paths,
            [PathBuf::from("/workspace/project/backend/src/generated")]
        );
    }

    #[test]
    fn path_escape_is_denied_before_approval() {
        let outcome = decide("rm -rf ../../outside", NetworkPolicy::Allow);
        assert_eq!(outcome.decision, ExecutionDecision::Deny);
        assert_eq!(outcome.code, "write_scope_escape");
    }

    #[test]
    fn command_admission_requests_external_access_but_file_tools_stay_scoped() {
        let root = tempfile::tempdir().unwrap();
        let canonical = root.path().canonicalize().unwrap();
        let root = canonical.as_path();
        let external = root.parent().unwrap().join("jarvis-external-target");
        let command = tool(
            "bash",
            serde_json::json!({"command":format!("cat '{}'", external.display())}),
        );
        let admitted = inspect_tool(root, &command, capabilities(Effect::Mutating))
            .unwrap()
            .unwrap();
        assert_eq!(admitted.outcome.decision, ExecutionDecision::Ask);
        assert!(admitted.outcome.native_working_directory.is_some());
        assert!(super::super::execution_sandbox::prepare(&admitted).is_some());
        let file = tool("read", serde_json::json!({"path":external}));
        assert_eq!(
            inspect_tool(root, &file, capabilities(Effect::ReadOnly))
                .unwrap()
                .unwrap()
                .outcome
                .decision,
            ExecutionDecision::Deny
        );
    }

    #[test]
    fn explicit_native_request_requires_reason_and_cannot_override_hard_policy() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        let mut command = tool(
            "bash",
            serde_json::json!({"command":"git status","sandboxPermissions":"require_escalated"}),
        );
        assert_eq!(
            inspect_tool(&root, &command, capabilities(Effect::Mutating))
                .unwrap_err()
                .code,
            "permission_justification_required"
        );
        command.args["justification"] = "Acesso ao recurso bloqueado".into();
        assert_eq!(
            inspect_tool(&root, &command, capabilities(Effect::Mutating))
                .unwrap()
                .unwrap()
                .outcome
                .code,
            "native_execution_approval_required"
        );
        command.args["command"] = "sudo touch /root/secret".into();
        let denied = inspect_tool(&root, &command, capabilities(Effect::Mutating))
            .unwrap()
            .unwrap();
        assert_eq!(denied.outcome.decision, ExecutionDecision::Deny);
        assert!(denied.outcome.native_working_directory.is_none());
    }

    #[test]
    fn nested_git_directory_options_do_not_turn_status_into_a_mutation() {
        let status = decide("git -C front status --short", NetworkPolicy::Deny);
        assert_eq!(status.decision, ExecutionDecision::Allow);
        assert!(status
            .read_paths
            .ends_with(&[PathBuf::from("/workspace/project/backend/front")]));
        let outside = decide("git -C ../../outside status", NetworkPolicy::Allow);
        assert_eq!(outside.code, "read_scope_escape");
    }

    #[test]
    fn network_follows_the_declared_scope_policy() {
        let denied = decide("git push origin main", NetworkPolicy::Deny);
        let asked = decide("git push origin main", NetworkPolicy::Ask);
        let allowed = decide("git push origin main", NetworkPolicy::Allow);
        assert_eq!(denied.decision, ExecutionDecision::Deny);
        assert_eq!(asked.decision, ExecutionDecision::Ask);
        assert_eq!(allowed.decision, ExecutionDecision::Allow);
    }

    #[test]
    fn project_scripts_and_opaque_commands_include_network_in_admission() {
        for command in [
            "npm test",
            "npm run check",
            "npm --prefix backend run build",
            "pnpm test",
            "yarn build",
            "bun run dev",
            "bun test",
            "cargo test --locked",
            "go test ./...",
            "node scripts/check.js",
            "python3 manage.py test",
            "php artisan migrate",
            "zsh -lc 'npm test'",
            "env NODE_ENV=test npm test",
            "psql -h localhost -c 'select 1'",
            "custom-test-runner",
            "TMPDIR=\"$PWD/.next/cache/test-tmp\" npm run check",
        ] {
            let requested = decide(command, NetworkPolicy::Ask);
            assert!(requested.effects.uses_network, "{command}");
            assert_eq!(requested.code, "network_approval_required", "{command}");
            assert_eq!(
                decide(command, NetworkPolicy::Deny).decision,
                ExecutionDecision::Deny,
                "{command} must not override an explicit network denial"
            );
            assert_ne!(
                decide(command, NetworkPolicy::Allow).decision,
                ExecutionDecision::Deny,
                "{command}"
            );
        }
    }

    #[test]
    fn known_local_queries_do_not_request_network() {
        for command in [
            "git status --short",
            "rg needle src",
            "node --version",
            "npm --version",
            "python3 --help",
            "cargo version",
            "go version",
        ] {
            let requested = decide(command, NetworkPolicy::Deny);
            assert!(!requested.effects.uses_network, "{command}");
            assert_ne!(requested.decision, ExecutionDecision::Deny, "{command}");
        }
    }

    #[test]
    fn dynamic_and_unknown_commands_never_inherit_read_only_allowance() {
        assert_eq!(
            decide("echo $(cat token)", NetworkPolicy::Allow).decision,
            ExecutionDecision::Ask
        );
        assert_eq!(
            decide("custom-tool --inspect", NetworkPolicy::Allow).decision,
            ExecutionDecision::Ask
        );
    }

    #[test]
    fn privilege_escalation_is_denied() {
        let outcome = decide("sudo git status", NetworkPolicy::Allow);
        assert_eq!(outcome.decision, ExecutionDecision::Deny);
        assert_eq!(outcome.code, "privilege_escalation_denied");
    }

    #[test]
    fn explicit_git_metadata_roots_cannot_be_hidden_by_a_local_work_tree() {
        let outcome = decide(
            "git --git-dir=/outside/.git --work-tree=. status",
            NetworkPolicy::Allow,
        );
        assert_eq!(outcome.code, "read_scope_escape");
        assert!(outcome.read_paths.contains(&PathBuf::from("/outside/.git")));
        let write = decide(
            "git -C front --git-dir=.git --work-tree=src add app.ts",
            NetworkPolicy::Allow,
        );
        assert!(write
            .write_paths
            .contains(&PathBuf::from("/workspace/project/backend/front/.git")));
        assert!(write
            .write_paths
            .contains(&PathBuf::from("/workspace/project/backend/front/src")));
    }

    #[test]
    fn read_only_capability_cannot_hide_mutation() {
        let root = Path::new("/workspace/project");
        let scope = ExecutionScope::project(root, root);
        let outcome = evaluate(PolicyRequest {
            tool_name: "read",
            capabilities: capabilities(Effect::ReadOnly),
            scope: &scope,
            operation: ExecutionOperation::Filesystem {
                read_paths: vec![],
                write_paths: vec![PathBuf::from("src/lib.rs")],
            },
        });
        assert_eq!(outcome.decision, ExecutionDecision::Deny);
        assert_eq!(outcome.code, "tool_capability_mismatch");
    }

    #[test]
    fn redirects_are_classified_by_operator() {
        let output = decide("rg todo src > report.txt", NetworkPolicy::Deny);
        assert_eq!(output.decision, ExecutionDecision::Ask);
        assert!(output.effects.writes_filesystem);
        assert!(output
            .write_paths
            .ends_with(&[PathBuf::from("/workspace/project/backend/report.txt")]));
    }

    #[cfg(unix)]
    #[test]
    fn null_device_redirections_do_not_require_filesystem_write_approval() {
        for command in [
            "git log --left-right --oneline main...origin/main > /dev/null",
            "git status 2>/dev/null",
            "printf discarded >> /dev/null",
            "cat README.md < /dev/null",
        ] {
            let outcome = decide(command, NetworkPolicy::Deny);
            assert_eq!(
                outcome.decision,
                ExecutionDecision::Allow,
                "{command}: {outcome:?}"
            );
            assert!(!outcome.effects.writes_filesystem, "{command}");
            assert!(outcome.write_paths.is_empty(), "{command}");
        }
    }

    #[test]
    fn null_device_exception_does_not_allow_other_paths_or_device_removal() {
        for command in [
            "printf data > /dev/null-backup",
            "printf data > /dev/zero",
            "printf data > /etc/jarvis-test",
            "printf data > ../../outside",
            "rm /dev/null",
            "mv /dev/null replacement",
        ] {
            assert_eq!(
                decide(command, NetworkPolicy::Deny).decision,
                ExecutionDecision::Deny,
                "{command}"
            );
        }
        let outcome = decide("printf data > dev/null", NetworkPolicy::Deny);
        assert_eq!(outcome.decision, ExecutionDecision::Ask);
        assert!(outcome.effects.writes_filesystem);
    }

    #[test]
    fn explicit_network_and_persistent_operations_are_classified() {
        let root = Path::new("/workspace/project");
        let mut scope = ExecutionScope::project(root, root);
        scope.network = NetworkPolicy::Ask;
        let network = evaluate(PolicyRequest {
            tool_name: "webhook",
            capabilities: capabilities(Effect::Mutating),
            scope: &scope,
            operation: ExecutionOperation::Network,
        });
        let process = evaluate(PolicyRequest {
            tool_name: "process_start",
            capabilities: capabilities(Effect::Stateful),
            scope: &scope,
            operation: ExecutionOperation::PersistentProcess(parse_command("bun run dev").unwrap()),
        });
        assert_eq!(network.code, "network_approval_required");
        assert_eq!(process.code, "network_approval_required");
    }

    #[test]
    fn common_tool_preflight_covers_shell_terminal_process_and_publication() {
        let root = tempfile::tempdir().unwrap();
        let canonical_root = std::fs::canonicalize(root.path()).unwrap();
        let stateful = capabilities(Effect::Stateful);
        let mutating = capabilities(Effect::Mutating);
        let shell = inspect_tool(
            &canonical_root,
            &tool("bash", serde_json::json!({"command":"git status"})),
            mutating,
        )
        .unwrap()
        .unwrap();
        let terminal = inspect_tool(
            &canonical_root,
            &tool("terminal_start", serde_json::json!({})),
            stateful,
        )
        .unwrap()
        .unwrap();
        let process = inspect_tool(
            &canonical_root,
            &tool(
                "process_start",
                serde_json::json!({"command":"bun run dev"}),
            ),
            stateful,
        )
        .unwrap()
        .unwrap();
        let publication = inspect_tool(
            &canonical_root,
            &tool(
                "jarvis_propose_publication",
                serde_json::json!({"repositories":[{"path":".","push":"normal","pullRequest":null,"reset":null}]}),
            ),
            capabilities(Effect::Interactive),
        )
        .unwrap()
        .unwrap();
        assert_eq!(shell.outcome.decision, ExecutionDecision::Allow);
        assert_eq!(terminal.outcome.code, "network_approval_required");
        assert_eq!(process.outcome.code, "network_approval_required");
        assert_eq!(publication.outcome.code, "network_approval_required");
    }

    #[test]
    fn publication_sync_declares_network_and_rebase_history_effects() {
        let root = tempfile::tempdir().unwrap();
        let sync = publication_operation(
            &serde_json::json!({"repositories":[{"path":"portal","sync":"ff_only","push":"none","pullRequest":null,"reset":null}]}),
            root.path(),
        );
        assert!(matches!(
            sync,
            ExecutionOperation::Declared {
                network: true,
                destructive: false,
                ..
            }
        ));
        let rebase = publication_operation(
            &serde_json::json!({"repositories":[{"path":"portal","sync":"rebase","push":"none","pullRequest":null,"reset":null}]}),
            root.path(),
        );
        assert!(matches!(
            rebase,
            ExecutionOperation::Declared {
                network: true,
                destructive: true,
                ..
            }
        ));
    }

    #[test]
    fn interactive_terminal_admission_covers_later_commands() {
        let root = tempfile::tempdir().unwrap();
        let policy = inspect_tool(
            root.path(),
            &tool(
                "terminal_start",
                serde_json::json!({"command":"git status"}),
            ),
            capabilities(Effect::Stateful),
        )
        .unwrap()
        .unwrap();
        assert!(policy.outcome.effects.uses_network);
        assert_eq!(policy.outcome.decision, ExecutionDecision::Ask);
    }
}
