use super::*;
use crate::persistence::initialize_database;

fn database() -> Connection {
    let mut database = Connection::open_in_memory().unwrap();
    initialize_database(&mut database).unwrap();
    database
        .execute(
            "INSERT INTO workspaces (id, name) VALUES ('w1', 'Pessoal')",
            [],
        )
        .unwrap();
    database
        .execute(
            "INSERT INTO projects (id, workspace_id, name, path) VALUES ('p1', 'w1', 'Jarvis', '/project')",
            [],
        )
        .unwrap();
    database
}

fn call(name: &str, command: &str) -> ToolCall {
    ToolCall {
        id: "tool-1".into(),
        name: name.into(),
        args: json!({"command":command}),
        status: "running".into(),
        output: String::new(),
        duration_ms: 0,
    }
}

fn repo_proposal(path: &str, files: &[&str]) -> RepositoryProposal {
    RepositoryProposal {
        path: path.into(),
        files: files.iter().map(|file| (*file).into()).collect(),
        branch: None,
        commit_message: "feat: publish approved change".into(),
        pull_request: None,
    }
}

fn git_ok<I, S>(directory: &Path, args: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    git(directory, args).unwrap()
}

fn repository() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    initialize_repository(directory.path());
    directory
}

fn initialize_repository(directory: &Path) {
    git_ok(directory, ["init"]);
    git_ok(directory, ["config", "user.name", "Jarvis Test"]);
    git_ok(directory, ["config", "user.email", "jarvis@example.test"]);
    std::fs::write(directory.join("app.txt"), "before\n").unwrap();
    std::fs::write(directory.join("outside.txt"), "stable\n").unwrap();
    git_ok(directory, ["add", "--all"]);
    git_ok(
        directory,
        ["commit", "--no-gpg-sign", "-m", "chore: initial"],
    );
}

#[test]
fn settings_default_to_local_commits_and_require_gh_for_pr_questions() {
    let mut database = database();
    assert_eq!(read(&database, "p1").unwrap(), SettingsInput::default());
    let enabled = SettingsInput {
        publish_prompt: "Review only related changes".into(),
        pr_mode: PullRequestMode::AskPr,
        pr_prompt: "Use the project template".into(),
    };
    assert_eq!(
        save(&mut database, "p1", enabled.clone(), false)
            .unwrap_err()
            .code,
        "github_cli_unavailable"
    );
    let stored = save(&mut database, "p1", enabled, true).unwrap();
    assert_eq!(stored.pr_mode, PullRequestMode::AskPr);
    assert_eq!(read(&database, "p1").unwrap(), stored);
    let local_only = save(
        &mut database,
        "p1",
        SettingsInput {
            publish_prompt: "Review local changes".into(),
            pr_mode: PullRequestMode::Disabled,
            pr_prompt: "  ".into(),
        },
        false,
    )
    .unwrap();
    assert_eq!(local_only.pr_prompt, DEFAULT_PR_PROMPT);
    database
        .execute("DELETE FROM projects WHERE id = 'p1'", [])
        .unwrap();
    assert_eq!(
        database
            .query_row(
                "SELECT count(*) FROM project_publication_settings",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn publication_tool_exposes_typed_multi_repository_review() {
    let definition = definition();
    assert_eq!(definition["name"], "jarvis_propose_publication");
    assert_eq!(
        definition["parameters"]["properties"]["repositories"]["maxItems"],
        8
    );
    let settings = Settings {
        project_id: "p1".into(),
        publish_prompt: "Use focused commits".into(),
        pr_mode: PullRequestMode::AskPrMerge,
        pr_prompt: "Use Summary and Validation".into(),
        gh_available: true,
    };
    let prompt = instructions(&settings);
    assert!(prompt.contains("jarvis_propose_publication"));
    assert!(prompt.contains("Never infer merge authorization"));
    assert!(prompt.contains(PR_QUESTION_ID));
    assert!(prompt.contains("Use focused commits"));
    assert_eq!(
        prompt_data("Use <scope> & keep it"),
        "Use &lt;scope&gt; &amp; keep it"
    );
}

#[test]
fn only_a_completed_non_cancelled_publication_question_satisfies_the_pr_preflight() {
    let mut question = ToolCall {
        id: "ask-1".into(),
        name: "ask_user".into(),
        args: json!({"questions":[{"id":PR_QUESTION_ID,"question":"Criar PR?"}]}),
        status: "completed".into(),
        output: json!({"cancelled":false,"answers":[{"id":PR_QUESTION_ID,"value":"Sim"}]})
            .to_string(),
        duration_ms: 10,
    };
    assert!(answered_publication_question(&question));
    question.args = json!({"questions":[{"id":"unrelated","question":"Outro assunto?"}]});
    assert!(!answered_publication_question(&question));
    question.args = json!({"questions":[{"id":PR_QUESTION_ID,"question":"Criar PR?"}]});
    question.output = json!({"cancelled":true,"answers":[]}).to_string();
    assert!(!answered_publication_question(&question));
}

#[test]
fn shell_publication_mutations_are_blocked_without_false_positives_for_inspection() {
    assert!(blocks_unsupervised_tool(&call("bash", "git status --short")).is_none());
    assert!(blocks_unsupervised_tool(&call("bash", "git diff --cached")).is_none());
    assert!(blocks_unsupervised_tool(&call("bash", "rg 'git commit' src")).is_none());
    assert!(blocks_unsupervised_tool(&call("bash", "git commit -m test")).is_some());
    assert!(blocks_unsupervised_tool(&call(
        "terminal_start",
        "cd app && git -C . push origin HEAD"
    ))
    .is_some());
    assert!(blocks_unsupervised_tool(&call("process_start", "gh pr merge 42 --squash")).is_some());
    assert!(
        blocks_unsupervised_tool(&call("bash", "gh --repo owner/project pr create --fill"))
            .is_some()
    );
    assert!(
        blocks_unsupervised_tool(&call("bash", "gh pr --repo owner/project merge 42")).is_some()
    );
    assert!(blocks_unsupervised_tool(&call("bash", "/usr/bin/env git commit -m test")).is_some());
    assert!(blocks_unsupervised_tool(&call("bash", "sudo -u root git push origin HEAD")).is_some());
    assert!(blocks_unsupervised_tool(&call("bash", "bash -lc 'gh pr create --fill'")).is_some());
    assert!(blocks_unsupervised_tool(&call("bash", "gh pr view 42")).is_none());
    assert_eq!(
        bounded("fatal: https://user:secret@example.test/repo?access_token=private Authorization: Bearer hidden"),
        "fatal: https://***@example.test/repo?access_token=*** Authorization: Bearer ***"
    );
    assert!(blocks_unsupervised_mcp(
        "github",
        "create_or_update_file",
        "Create or update a file in a GitHub repository"
    )
    .is_some());
    assert!(blocks_unsupervised_mcp(
        "source-control",
        "create_pull_request",
        "Create a pull request"
    )
    .is_some());
    assert!(blocks_unsupervised_mcp(
        "database",
        "commit_transaction",
        "Commit a database transaction"
    )
    .is_none());
}

#[test]
fn approved_local_publication_commits_only_the_reviewed_files() {
    let repository = repository();
    let root = std::fs::canonicalize(repository.path()).unwrap();
    std::fs::write(repository.path().join("app.txt"), "after\n").unwrap();
    std::fs::write(repository.path().join("outside.txt"), "unreviewed\n").unwrap();
    let result = publish_repository(&root, &repo_proposal(".", &["app.txt"]), false).unwrap();
    assert!(result.commit.is_some());
    assert!(result.pull_request.is_none());
    assert_eq!(
        git_ok(
            repository.path(),
            ["show", "--pretty=format:", "--name-only", "HEAD"]
        ),
        "app.txt"
    );
    assert!(git_ok(repository.path(), ["status", "--short"])
        .lines()
        .any(|line| line.ends_with("outside.txt")));
}

#[test]
fn approved_publication_resolves_a_nested_repository_from_the_project_root() {
    let project = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(project.path()).unwrap();
    let backend = root.join("backend");
    std::fs::create_dir(&backend).unwrap();
    initialize_repository(&backend);
    std::fs::write(backend.join("app.txt"), "after\n").unwrap();
    let result = publish_repository(&root, &repo_proposal("backend", &["app.txt"]), false).unwrap();
    assert!(result.commit.is_some());
    assert_eq!(
        git_ok(
            &backend,
            ["show", "--pretty=format:", "--name-only", "HEAD"]
        ),
        "app.txt"
    );
}

#[test]
fn proposal_rejects_staged_files_that_are_missing_from_the_drawer() {
    let repository = repository();
    let root = std::fs::canonicalize(repository.path()).unwrap();
    std::fs::write(repository.path().join("app.txt"), "after\n").unwrap();
    std::fs::write(repository.path().join("outside.txt"), "staged\n").unwrap();
    git_ok(repository.path(), ["add", "outside.txt"]);
    let failure = validate_repository(&root, &repo_proposal(".", &["app.txt"]), false).unwrap_err();
    assert_eq!(failure.code, "publication_staged_scope");
}

#[test]
fn proposal_requires_literal_changed_files_instead_of_a_directory_pathspec() {
    let repository = repository();
    let root = std::fs::canonicalize(repository.path()).unwrap();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(root.join("src/app.txt"), "new\n").unwrap();
    let failure = validate_repository(&root, &repo_proposal(".", &["src"]), false).unwrap_err();
    assert_eq!(failure.code, "invalid_publication_proposal");
    assert!(failure.message.contains("não possui uma alteração Git"));
}
