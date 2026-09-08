//! Explicit user-facing reads and comment writes against the same private tracker
//! as the agent. These commands never initialize a database or change an issue.
use super::*;
use crate::{library, persistence::AppState};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Issue {
    pub id: String,
    pub title: String,
    pub description: String,
    pub design: String,
    pub acceptance_criteria: String,
    pub notes: String,
    pub status: String,
    pub priority: u8,
    pub issue_type: String,
    pub assignee: String,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
    pub closed_at: Option<String>,
    pub close_reason: String,
    pub labels: Vec<String>,
    pub dependencies: Vec<Relation>,
    pub dependents: Vec<Relation>,
    pub comment_count: u64,
    pub parent: Option<String>,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Relation {
    pub id: String,
    pub title: String,
    pub status: String,
    pub dependency_type: String,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Comment {
    #[serde(deserialize_with = "comment_id")]
    pub id: String,
    pub author: String,
    pub text: String,
    pub created_at: String,
}
fn comment_id<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::String(value) if !value.is_empty() => Ok(value),
        Value::Number(value) if value.is_u64() => Ok(value.to_string()),
        _ => Err(serde::de::Error::custom(
            "Identificador de comentário inválido",
        )),
    }
}
#[derive(Serialize)]
pub struct Detail {
    issue: Issue,
    comments: Vec<Comment>,
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, CoreError> {
    serde_json::from_value(value).map_err(|cause| {
        failure(format!(
            "O Beads retornou dados incompatíveis com o Dashboard: {cause}"
        ))
    })
}

fn live(state: &AppState, home: &Path, project: &str) -> Result<(), CoreError> {
    library::dashboard::check_project(state, home, project)
        .map_err(|_| failure("Este projeto não está mais disponível."))
}

impl Beads {
    async fn board(&self, signal: watch::Receiver<bool>) -> Result<Vec<Issue>, CoreError> {
        if !self.validate_store()? {
            return Ok(vec![]);
        }
        // Embedded bd has no offset pagination. Read all statuses, including
        // pinned/hooked, without the CLI's default 50-item truncation. The shared
        // process output bound fails explicitly instead of returning partial data.
        let value = self
            .run(
                &["list", "--status=all", "--limit=0"].map(String::from),
                false,
                signal,
            )
            .await?;
        let mut issues: Vec<Issue> = decode(value)?;
        issues.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| b.updated_at.cmp(&a.updated_at))
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(issues)
    }
    async fn detail(&self, id: &str, signal: watch::Receiver<bool>) -> Result<Detail, CoreError> {
        tools::issue_id(id, &self.prefix())?;
        if !self.validate_store()? {
            return Err(failure("Tarefa não encontrada."));
        }
        let mut issues: Vec<Issue> = decode(
            self.run(
                &["show".into(), id.into(), "--include-dependents".into()],
                false,
                signal.clone(),
            )
            .await?,
        )?;
        let issue = issues
            .pop()
            .ok_or_else(|| failure("Tarefa não encontrada."))?;
        let comments: Vec<Comment> = decode(
            self.run(&["comments".into(), id.into()], false, signal)
                .await?,
        )?;
        Ok(Detail { issue, comments })
    }
}

#[tauri::command]
pub async fn get_project_beads(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<Issue>, CoreError> {
    let home = app
        .path()
        .home_dir()
        .map_err(|_| failure("Pasta pessoal indisponível."))?;
    let beads = Beads::new(&home, &project_id, &project_id, true)?;
    let (_sender, signal) = watch::channel(false);
    let _lock = beads.lock(signal.clone()).await?;
    live(&state, &home, &project_id)?;
    beads.board(signal).await
}

#[tauri::command]
pub async fn get_bead_detail(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    project_id: String,
    issue_id: String,
) -> Result<Detail, CoreError> {
    let home = app
        .path()
        .home_dir()
        .map_err(|_| failure("Pasta pessoal indisponível."))?;
    let beads = Beads::new(&home, &project_id, &project_id, true)?;
    tools::issue_id(&issue_id, &beads.prefix())?;
    let (_sender, signal) = watch::channel(false);
    let _lock = beads.lock(signal.clone()).await?;
    live(&state, &home, &project_id)?;
    beads.detail(&issue_id, signal).await
}

fn comment_args(id: &str, text: &str, prefix: &str) -> Result<Vec<String>, CoreError> {
    tools::issue_id(id, prefix)?;
    let text = text.trim();
    if text.is_empty() || text.chars().count() > 10_000 || text.contains('\0') {
        return Err(failure("Escreva um comentário de até 10.000 caracteres."));
    }
    Ok(vec![
        "comments".into(),
        "add".into(),
        "--author=Você".into(),
        "--".into(),
        id.into(),
        text.into(),
    ])
}

#[tauri::command]
pub async fn add_bead_comment(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    project_id: String,
    issue_id: String,
    text: String,
) -> Result<Comment, CoreError> {
    let home = app
        .path()
        .home_dir()
        .map_err(|_| failure("Pasta pessoal indisponível."))?;
    let beads = Beads::new(&home, &project_id, &project_id, false)?;
    let args = comment_args(&issue_id, &text, &beads.prefix())?;
    let (_sender, signal) = watch::channel(false);
    let _lock = beads.lock(signal.clone()).await?;
    live(&state, &home, &project_id)?;
    if !beads.validate_store()? {
        return Err(failure("Tarefa não encontrada."));
    }
    // Place global flags before the command's -- delimiter, which keeps even a
    // comment beginning with dashes as literal positional text.
    let mut command = process::command(&beads.package, &beads.workspace(), &beads.session);
    command.args(["--json", "--dolt-auto-commit=on"]).args(args);
    let comment: Comment = decode(
        serde_json::from_str(&process::run(command, signal).await?).map_err(|_| {
            failure("Resposta inválida. Confira os comentários antes de repetir o envio.")
        })?,
    )?;
    let _ = app.emit("beads:changed", &project_id);
    Ok(comment)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comments_validate_scope_and_keep_text_literal() {
        assert!(comment_args("other-1", "a", "j123").is_err());
        assert!(comment_args("j123-1", "  ", "j123").is_err());
        assert!(comment_args("j123-1", &"a".repeat(10_001), "j123").is_err());
        let args = comment_args("j123-1", "--delete $(touch file)\nOlá", "j123").unwrap();
        assert_eq!(args.last().unwrap(), "--delete $(touch file)\nOlá");
        assert_eq!(args[3], "--");
    }

    #[tokio::test]
    #[ignore = "Uses installed private Beads in an isolated temporary home"]
    async fn installed_dashboard_reads_all_statuses_relations_and_persists_comments() {
        let home = tempfile::tempdir().unwrap();
        let host = PathBuf::from(
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .unwrap(),
        );
        let beads = Beads {
            package: super::super::super::installed(&host, ComponentId::Beads)
                .unwrap()
                .path(&host)
                .unwrap(),
            home: home.path().into(),
            project: "0123456789abcdef0123456789abcdef".into(),
            session: "fedcba9876543210fedcba9876543210".into(),
            plan: false,
        };
        let (_sender, signal) = watch::channel(false);
        assert!(beads.board(signal.clone()).await.unwrap().is_empty());
        assert!(!beads.workspace().exists());
        private_root(home.path()).unwrap();
        beads.initialize(signal.clone()).await.unwrap();
        let mut first = String::new();
        for (index, status) in [
            "open",
            "in_progress",
            "blocked",
            "deferred",
            "closed",
            "pinned",
            "hooked",
        ]
        .iter()
        .enumerate()
        {
            let args = vec![
                "create".into(),
                format!("--title=Dashboard {status}"),
                format!("--type={}", if index == 0 { "epic" } else { "task" }),
                "--description=Native Dashboard validation".into(),
            ];
            let value = beads.run(&args, true, signal.clone()).await.unwrap();
            let id = value["id"].as_str().unwrap().to_string();
            if index == 0 {
                first = id.clone();
            } else {
                beads
                    .run(
                        &["update".into(), id.clone(), format!("--status={status}")],
                        true,
                        signal.clone(),
                    )
                    .await
                    .unwrap();
                beads
                    .run(
                        &[
                            "dep".into(),
                            "add".into(),
                            id,
                            first.clone(),
                            "--type=parent-child".into(),
                        ],
                        true,
                        signal.clone(),
                    )
                    .await
                    .unwrap();
            }
        }
        let board = beads.board(signal.clone()).await.unwrap();
        assert_eq!(board.len(), 7);
        for status in [
            "open",
            "in_progress",
            "blocked",
            "deferred",
            "closed",
            "pinned",
            "hooked",
        ] {
            assert!(board.iter().any(|issue| issue.status == status));
        }
        let detail = beads.detail(&first, signal.clone()).await.unwrap();
        assert_eq!(detail.issue.dependents.len(), 6);
        let mut command = process::command(&beads.package, &beads.workspace(), &beads.session);
        command.args(["--json", "--dolt-auto-commit=on"]).args(
            comment_args(
                &first,
                "--Comentário de teste\nPersistido sem editar a tarefa.",
                &beads.prefix(),
            )
            .unwrap(),
        );
        let value: Value =
            serde_json::from_str(&process::run(command, signal.clone()).await.unwrap()).unwrap();
        let comment: Comment = decode(value).unwrap();
        let updated = beads.detail(&first, signal.clone()).await.unwrap();
        assert_eq!(updated.comments.len(), 1);
        assert_eq!(updated.comments[0].id, comment.id);
        assert_eq!(updated.comments[0].author, "Você");
        assert!(updated.comments[0].text.starts_with("--Comentário"));
        assert_eq!(updated.issue.status, detail.issue.status);
        assert_eq!(updated.issue.description, detail.issue.description);
        let other = Beads {
            project: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            ..beads
        };
        assert!(other.board(signal.clone()).await.unwrap().is_empty());
        assert!(other.detail(&first, signal).await.is_err());
    }
}
