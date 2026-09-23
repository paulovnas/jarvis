use super::*;
use crate::persistence::initialize_database;

#[test]
fn publication_contract_reuses_evidence_and_does_not_expand_into_a_whole_project_audit() {
    assert!(DEFAULT_PUBLISH_PROMPT.contains("Reuse valid checks already performed"));
    assert!(DEFAULT_PUBLISH_PROMPT.contains("whole-codebase audit"));
    assert!(DEFAULT_PUBLISH_PROMPT.contains("ask only for a material choice"));
    let contract =
        crate::agent::workflow::catalog::builtin_agent(crate::agent::workflow::Role::Github)
            .unwrap()
            .instructions;
    assert!(contract.contains("jarvis_inspect_publication"));
    assert!(contract.contains("Inspect the relevant diff once"));
    assert!(contract.contains("do not repeat an uncertain action"));
    assert!(contract.contains("never ask the user to decide it again"));
    assert!(contract.contains("autonomous"));
    assert!(contract.contains("Recoverable errors"));
    assert!(contract.contains("include sync"));
    assert!(contract.contains("Never infer a push"));
}

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
        reset: None,
        files: files.iter().map(|file| (*file).into()).collect(),
        branch: None,
        commit_message: Some("feat: publish approved change".into()),
        sync: SyncMode::None,
        push: PushMode::None,
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

fn linked_hml_repositories() -> (tempfile::TempDir, tempfile::TempDir, tempfile::TempDir) {
    let local = repository();
    let remote = tempfile::tempdir().unwrap();
    let peer = tempfile::tempdir().unwrap();
    git_ok(local.path(), ["branch", "-M", "hml"]);
    git_ok(remote.path(), ["init", "--bare"]);
    git_ok(remote.path(), ["symbolic-ref", "HEAD", "refs/heads/hml"]);
    git_ok(
        local.path(),
        ["remote", "add", "origin", remote.path().to_str().unwrap()],
    );
    git_ok(local.path(), ["push", "--set-upstream", "origin", "hml"]);
    git_ok(peer.path(), ["clone", remote.path().to_str().unwrap(), "."]);
    git_ok(peer.path(), ["config", "user.name", "Jarvis Peer"]);
    git_ok(peer.path(), ["config", "user.email", "peer@example.test"]);
    (local, remote, peer)
}

fn advance_remote(peer: &Path, file: &str, content: &str) -> String {
    std::fs::write(peer.join(file), content).unwrap();
    git_ok(peer, ["add", "--", file]);
    git_ok(
        peer,
        ["commit", "--no-gpg-sign", "-m", "feat: remote change"],
    );
    git_ok(peer, ["push", "origin", "hml"]);
    git_ok(peer, ["rev-parse", "HEAD"])
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
    let repository = &definition["parameters"]["properties"]["repositories"]["items"];
    assert_eq!(repository["properties"]["files"]["minItems"], 0);
    assert_eq!(
        repository["properties"]["reset"]["anyOf"][1]["properties"]["mode"]["enum"][0],
        "soft"
    );
    assert_eq!(
        repository["properties"]["push"]["enum"][2],
        "force_with_lease"
    );
    assert_eq!(repository["properties"]["sync"]["enum"][1], "ff_only");
    assert_eq!(repository["properties"]["sync"]["enum"][2], "rebase");
    assert_eq!(
        definition["parameters"]["properties"]["authorization"]["anyOf"][1]["properties"]["mode"]
            ["enum"][1],
        "autonomous"
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
    assert!(prompt.contains("authority matrix"));
    assert!(prompt.contains("do not ask whether to perform them again"));
    assert!(prompt.contains("without another review"));
    assert!(prompt.contains("Never send the user to a terminal"));
    assert!(prompt.contains("sync=ff_only"));
    assert!(prompt.contains("Sync does not imply push"));
    assert!(prompt.contains("reused automatically"));
    assert!(prompt.contains("revision_requested"));
    assert!(prompt.contains("submit a revised proposal"));
    assert!(prompt.contains(PR_QUESTION_ID));
    assert!(prompt.contains("Use focused commits"));
    assert_eq!(
        prompt_data("Use <scope> & keep it"),
        "Use &lt;scope&gt; &amp; keep it"
    );
}

fn authorized_proposal(
    mode: UserAuthorizationMode,
    evidence: &str,
    repository: RepositoryProposal,
) -> Proposal {
    Proposal {
        summary: "Executar publicação solicitada".into(),
        authorization: Some(UserAuthorization {
            mode,
            evidence: evidence.into(),
        }),
        repositories: vec![repository],
    }
}

#[test]
fn explicit_current_request_avoids_redundant_pr_question_but_keeps_review() {
    let user = "Faça commit e push de tudo que está pendente.";
    let mut repository = repo_proposal(".", &["app.txt"]);
    repository.push = PushMode::Normal;
    let proposal = authorized_proposal(UserAuthorizationMode::ExplicitRequest, user, repository);

    validate_user_authorization(&proposal, user).unwrap();

    assert!(!requires_publication_question(
        PullRequestMode::AskPrMerge,
        &proposal,
        false
    ));
    assert!(!executes_without_review(&proposal));
}

#[test]
fn a_request_to_update_hml_locally_authorizes_sync_without_push() {
    let user = "Crie o commit, atualize a hml local com a remota pq acho que ta atras";
    let mut repository = repo_proposal(".", &["app.txt"]);
    repository.branch = Some("hml".into());
    repository.sync = SyncMode::Rebase;
    let proposal = authorized_proposal(UserAuthorizationMode::ExplicitRequest, user, repository);

    validate_user_authorization(&proposal, user).unwrap();
    assert!(!proposed_operations(&proposal).contains(&AuthorizedOperation::Push));
    assert!(!executes_without_review(&proposal));
}

#[test]
fn a_pull_request_or_fetch_only_does_not_authorize_local_integration() {
    let mut repository = repo_proposal(".", &[]);
    repository.commit_message = None;
    repository.sync = SyncMode::Rebase;
    for user in ["Crie uma pull request para hml", "Faça fetch de origin/hml"] {
        let proposal = authorized_proposal(
            UserAuthorizationMode::ExplicitRequest,
            user,
            repository.clone(),
        );
        let failure = validate_user_authorization(&proposal, user).unwrap_err();
        assert!(failure.message.contains("sincronização"));
    }
}

#[test]
fn autonomous_current_request_executes_all_named_operations_without_another_review() {
    let user = "Faça commit, push, PR e merge; pode executar direto, sem me perguntar novamente.";
    let mut repository = repo_proposal(".", &["app.txt"]);
    repository.push = PushMode::Normal;
    repository.pull_request = Some(PullRequestProposal {
        base: "main".into(),
        title: "Publicar alteração".into(),
        body: "Alteração solicitada e validada.".into(),
        draft: false,
        merge: Some(MergeProposal {
            method: MergeMethod::Squash,
            delete_branch: false,
        }),
    });
    let proposal = authorized_proposal(UserAuthorizationMode::Autonomous, user, repository);

    validate_user_authorization(&proposal, user).unwrap();

    assert!(executes_without_review(&proposal));
    assert!(!requires_publication_question(
        PullRequestMode::AskPrMerge,
        &proposal,
        false
    ));
}

#[test]
fn autonomous_soft_reset_uses_the_typed_action_when_the_user_named_it() {
    let user = "Execute git reset --soft HEAD^ sem me perguntar novamente.";
    let proposal = authorized_proposal(
        UserAuthorizationMode::Autonomous,
        user,
        RepositoryProposal {
            path: "movart-express-back".into(),
            reset: Some(ResetProposal {
                mode: ResetMode::Soft,
                target: "HEAD^".into(),
            }),
            files: vec![],
            branch: None,
            commit_message: None,
            sync: SyncMode::None,
            push: PushMode::None,
            pull_request: None,
        },
    );

    validate_user_authorization(&proposal, user).unwrap();
    assert!(executes_without_review(&proposal));
}

#[test]
fn autonomous_mode_rejects_missing_scope_or_a_non_verbatim_claim() {
    let user = "Faça commit e push sem me perguntar novamente.";
    let mut repository = repo_proposal(".", &["app.txt"]);
    repository.push = PushMode::Normal;
    repository.pull_request = Some(PullRequestProposal {
        base: "main".into(),
        title: "Publicar alteração".into(),
        body: "Alteração solicitada e validada.".into(),
        draft: false,
        merge: Some(MergeProposal {
            method: MergeMethod::Squash,
            delete_branch: false,
        }),
    });
    let proposal = authorized_proposal(UserAuthorizationMode::Autonomous, user, repository);
    let error = validate_user_authorization(&proposal, user).unwrap_err();
    assert_eq!(error.code, "invalid_publication_authorization");
    assert!(error.message.contains("pull request"));
    assert!(error.message.contains("merge"));

    let mut invalid_quote = proposal;
    invalid_quote.authorization.as_mut().unwrap().evidence =
        "Faça também a pull request e o merge.".into();
    let error = validate_user_authorization(&invalid_quote, user).unwrap_err();
    assert_eq!(error.code, "invalid_publication_authorization");
    assert!(error.message.contains("literalmente"));
}

#[test]
fn autonomy_accepts_delegated_judgment_but_rejects_an_explicit_review_caveat() {
    let autonomous = "Faça commit e push; pode fazer o que for necessário e use seu julgamento.";
    let mut repository = repo_proposal(".", &["app.txt"]);
    repository.push = PushMode::Normal;
    let proposal = authorized_proposal(
        UserAuthorizationMode::Autonomous,
        autonomous,
        repository.clone(),
    );
    validate_user_authorization(&proposal, autonomous).unwrap();

    let review =
        "Faça commit e push, você decide os detalhes, mas confirme antes para minha aprovação.";
    let proposal = authorized_proposal(UserAuthorizationMode::Autonomous, review, repository);
    let error = validate_user_authorization(&proposal, review).unwrap_err();
    assert_eq!(error.code, "invalid_publication_authorization");
    assert!(error.message.contains("dispense explicitamente"));
}

#[test]
fn configured_pr_question_remains_required_when_the_current_request_has_no_decision() {
    let proposal = Proposal {
        summary: "Publicar alteração".into(),
        authorization: None,
        repositories: vec![repo_proposal(".", &["app.txt"])],
    };

    assert!(requires_publication_question(
        PullRequestMode::AskPr,
        &proposal,
        false
    ));
    assert!(!requires_publication_question(
        PullRequestMode::AskPr,
        &proposal,
        true
    ));
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
    assert!(blocks_unsupervised_tool(&call("bash", "git fetch origin hml")).is_some());
    assert!(blocks_unsupervised_tool(&call("bash", "git pull --rebase origin hml")).is_some());
    assert!(blocks_unsupervised_tool(&call("bash", "git rebase origin/hml")).is_some());
    assert!(blocks_unsupervised_tool(&call(
        "bash",
        "git -C movart-express-back reset --soft HEAD^"
    ))
    .is_some());
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
fn approved_action_only_soft_reset_keeps_the_changes_staged() {
    let repository = repository();
    let root = std::fs::canonicalize(repository.path()).unwrap();
    let previous = git_ok(repository.path(), ["rev-parse", "HEAD"]);
    std::fs::write(repository.path().join("app.txt"), "after\n").unwrap();
    git_ok(repository.path(), ["add", "app.txt"]);
    git_ok(
        repository.path(),
        ["commit", "--no-gpg-sign", "-m", "feat: second"],
    );
    let proposal = RepositoryProposal {
        path: ".".into(),
        reset: Some(ResetProposal {
            mode: ResetMode::Soft,
            target: "HEAD^".into(),
        }),
        files: vec![],
        branch: None,
        commit_message: None,
        sync: SyncMode::None,
        push: PushMode::None,
        pull_request: None,
    };

    let result = publish_repository(&root, &proposal, false).unwrap();

    assert_eq!(git_ok(repository.path(), ["rev-parse", "HEAD"]), previous);
    assert_eq!(
        git_ok(repository.path(), ["diff", "--cached", "--name-only"]),
        "app.txt"
    );
    assert_eq!(result.reset.unwrap().target, "HEAD^");
    assert!(result.commit.is_none());
}

#[test]
fn approved_publication_can_select_an_existing_branch() {
    let repository = repository();
    let root = std::fs::canonicalize(repository.path()).unwrap();
    let original = current_branch(repository.path()).unwrap();
    git_ok(repository.path(), ["branch", "release"]);
    let proposal = RepositoryProposal {
        path: ".".into(),
        reset: None,
        files: vec![],
        branch: Some("release".into()),
        commit_message: None,
        sync: SyncMode::None,
        push: PushMode::None,
        pull_request: None,
    };

    publish_repository(&root, &proposal, false).unwrap();

    assert_eq!(current_branch(repository.path()).unwrap(), "release");
    assert_ne!(original, "release");
}

#[test]
fn approved_push_does_not_require_a_pull_request() {
    let repository = repository();
    let remote = tempfile::tempdir().unwrap();
    git_ok(remote.path(), ["init", "--bare"]);
    git_ok(
        repository.path(),
        ["remote", "add", "origin", remote.path().to_str().unwrap()],
    );
    std::fs::write(repository.path().join("app.txt"), "after\n").unwrap();
    let root = std::fs::canonicalize(repository.path()).unwrap();
    let mut proposal = repo_proposal(".", &["app.txt"]);
    proposal.push = PushMode::Normal;

    let result = publish_repository(&root, &proposal, false).unwrap();

    let branch = current_branch(repository.path()).unwrap();
    let remote_commit = git_ok(
        repository.path(),
        ["ls-remote", "origin", &format!("refs/heads/{branch}")],
    );
    assert!(remote_commit.starts_with(result.commit.as_deref().unwrap()));
    assert_eq!(result.push, PushMode::Normal);
    assert!(result.pull_request.is_none());
}

#[test]
fn approved_sync_fast_forwards_hml_without_pushing() {
    let (local, _remote, peer) = linked_hml_repositories();
    let remote_commit = advance_remote(peer.path(), "remote.txt", "remote change\n");
    let root = std::fs::canonicalize(local.path()).unwrap();
    let mut proposal = repo_proposal(".", &[]);
    proposal.commit_message = None;
    proposal.branch = Some("hml".into());
    proposal.sync = SyncMode::FfOnly;

    let result = publish_repository(&root, &proposal, false).unwrap();
    let sync = result.sync.unwrap();

    assert!(matches!(sync.outcome, SyncOutcome::FastForwarded));
    assert_eq!(sync.remote_branch, "origin/hml");
    assert_eq!(git_ok(local.path(), ["rev-parse", "HEAD"]), remote_commit);
    assert_eq!(result.push, PushMode::None);
}

#[test]
fn approved_commit_and_sync_rebase_preserve_local_work_on_new_remote_history() {
    let (local, remote, peer) = linked_hml_repositories();
    let remote_commit = advance_remote(peer.path(), "remote.txt", "remote change\n");
    std::fs::write(local.path().join("app.txt"), "local change\n").unwrap();
    let root = std::fs::canonicalize(local.path()).unwrap();
    let mut proposal = repo_proposal(".", &["app.txt"]);
    proposal.branch = Some("hml".into());
    proposal.sync = SyncMode::Rebase;

    let result = publish_repository(&root, &proposal, false).unwrap();
    let head = git_ok(local.path(), ["rev-parse", "HEAD"]);
    let sync = result.sync.unwrap();

    assert!(matches!(sync.outcome, SyncOutcome::Rebased));
    assert_eq!(result.commit.as_deref(), Some(head.as_str()));
    assert!(is_ancestor(local.path(), &remote_commit, &head).unwrap());
    assert_eq!(
        std::fs::read_to_string(local.path().join("app.txt")).unwrap(),
        "local change\n"
    );
    assert_eq!(
        git_ok(remote.path(), ["rev-parse", "refs/heads/hml"]),
        remote_commit
    );
}

#[test]
fn fast_forward_sync_refuses_divergence_without_rewriting_local_commits() {
    let (local, _remote, peer) = linked_hml_repositories();
    advance_remote(peer.path(), "remote.txt", "remote change\n");
    std::fs::write(local.path().join("app.txt"), "local change\n").unwrap();
    git_ok(local.path(), ["add", "app.txt"]);
    git_ok(
        local.path(),
        ["commit", "--no-gpg-sign", "-m", "feat: local change"],
    );
    let before = git_ok(local.path(), ["rev-parse", "HEAD"]);
    let root = std::fs::canonicalize(local.path()).unwrap();
    let mut proposal = repo_proposal(".", &[]);
    proposal.commit_message = None;
    proposal.sync = SyncMode::FfOnly;

    let failure = publish_repository(&root, &proposal, false).err().unwrap();

    assert_eq!(failure.code, "publication_sync_diverged");
    assert_eq!(git_ok(local.path(), ["rev-parse", "HEAD"]), before);
    assert_eq!(
        std::fs::read_to_string(local.path().join("app.txt")).unwrap(),
        "local change\n"
    );
}

#[test]
fn conflicted_rebase_is_aborted_and_preserves_local_commit() {
    let (local, _remote, peer) = linked_hml_repositories();
    advance_remote(peer.path(), "app.txt", "remote change\n");
    std::fs::write(local.path().join("app.txt"), "local change\n").unwrap();
    git_ok(local.path(), ["add", "app.txt"]);
    git_ok(
        local.path(),
        ["commit", "--no-gpg-sign", "-m", "feat: local change"],
    );
    let before = git_ok(local.path(), ["rev-parse", "HEAD"]);
    let root = std::fs::canonicalize(local.path()).unwrap();
    let mut proposal = repo_proposal(".", &[]);
    proposal.commit_message = None;
    proposal.sync = SyncMode::Rebase;

    let failure = publish_repository(&root, &proposal, false).err().unwrap();

    assert_eq!(failure.code, "publication_sync_conflict");
    assert!(failure.message.contains(&before));
    assert_eq!(git_ok(local.path(), ["rev-parse", "HEAD"]), before);
    assert_eq!(
        std::fs::read_to_string(local.path().join("app.txt")).unwrap(),
        "local change\n"
    );
    assert!(!rebase_in_progress(local.path()).unwrap());
}

#[test]
fn a_sync_failure_after_commit_reports_the_created_commit_for_recovery() {
    let (local, _remote, peer) = linked_hml_repositories();
    advance_remote(peer.path(), "app.txt", "remote change\n");
    std::fs::write(local.path().join("app.txt"), "local change\n").unwrap();
    let root = std::fs::canonicalize(local.path()).unwrap();
    let mut proposal = repo_proposal(".", &["app.txt"]);
    proposal.sync = SyncMode::Rebase;

    let failure = publish_repository(&root, &proposal, false).err().unwrap();
    let created = git_ok(local.path(), ["rev-parse", "HEAD"]);

    assert_eq!(failure.code, "publication_sync_conflict");
    assert!(failure.message.contains(&created));
    assert_eq!(
        std::fs::read_to_string(local.path().join("app.txt")).unwrap(),
        "local change\n"
    );
    assert!(!rebase_in_progress(local.path()).unwrap());
}

#[test]
fn a_commit_followed_by_sync_conflict_is_reported_as_partial_work() {
    let (local, _remote, peer) = linked_hml_repositories();
    advance_remote(peer.path(), "app.txt", "remote change\n");
    std::fs::write(local.path().join("app.txt"), "local change\n").unwrap();
    let root = std::fs::canonicalize(local.path()).unwrap();
    let before = git_ok(local.path(), ["rev-parse", "HEAD"]);
    let mut repository = repo_proposal(".", &["app.txt"]);
    repository.sync = SyncMode::Rebase;
    let proposal = Proposal {
        summary: "Commit e sincronização da hml".into(),
        authorization: None,
        repositories: vec![repository],
    };

    let result: Value = serde_json::from_str(&apply(&root, &proposal, None)).unwrap();

    assert_eq!(result["status"], "partial");
    assert_eq!(result["error"]["code"], "publication_sync_conflict");
    assert_eq!(result["failedRepository"]["before"]["head"], before);
    assert_eq!(
        result["failedRepository"]["after"]["head"],
        git_ok(local.path(), ["rev-parse", "HEAD"])
    );
}

struct ExistingPullRequestGithub {
    pull_request: PullRequestState,
    created: std::cell::Cell<usize>,
    merges: std::cell::RefCell<Vec<(String, String)>>,
}

impl GithubClient for ExistingPullRequestGithub {
    fn authenticated(&self, _directory: &Path) -> Result<(), AgentError> {
        Ok(())
    }

    fn find_open(
        &self,
        _directory: &Path,
        _base: &str,
        _head: &str,
    ) -> Result<Option<PullRequestState>, AgentError> {
        Ok(Some(self.pull_request.clone()))
    }

    fn create(
        &self,
        _directory: &Path,
        _proposal: &PullRequestProposal,
        _head: &str,
    ) -> Result<PullRequestState, AgentError> {
        self.created.set(self.created.get() + 1);
        Err(AgentError::internal())
    }

    fn merge(
        &self,
        _directory: &Path,
        pull_request: &PullRequestState,
        _proposal: &MergeProposal,
    ) -> Result<(), AgentError> {
        self.merges
            .borrow_mut()
            .push((pull_request.url.clone(), pull_request.head_commit.clone()));
        Ok(())
    }
}

#[test]
fn approved_merge_reuses_an_existing_pull_request_without_creating_a_duplicate() {
    let repository = repository();
    git_ok(
        repository.path(),
        [
            "remote",
            "add",
            "origin",
            "https://example.test/owner/project.git",
        ],
    );
    let root = std::fs::canonicalize(repository.path()).unwrap();
    let head = git_ok(repository.path(), ["rev-parse", "HEAD"]);
    let github = ExistingPullRequestGithub {
        pull_request: PullRequestState {
            url: "https://github.test/owner/project/pull/42".into(),
            head_commit: head.clone(),
        },
        created: std::cell::Cell::new(0),
        merges: std::cell::RefCell::new(vec![]),
    };
    let proposal = RepositoryProposal {
        path: ".".into(),
        reset: None,
        files: vec![],
        branch: None,
        commit_message: None,
        sync: SyncMode::None,
        push: PushMode::None,
        pull_request: Some(PullRequestProposal {
            base: "hml".into(),
            title: "PR existente".into(),
            body: "Reutilizar a PR aberta e concluir o merge aprovado.".into(),
            draft: false,
            merge: Some(MergeProposal {
                method: MergeMethod::Squash,
                delete_branch: true,
            }),
        }),
    };

    let result = publish_repository_with(&root, &proposal, Some(&github)).unwrap();

    assert_eq!(github.created.get(), 0);
    assert_eq!(
        github.merges.borrow().as_slice(),
        &[("https://github.test/owner/project/pull/42".into(), head,)]
    );
    assert_eq!(
        result.pull_request.as_deref(),
        Some("https://github.test/owner/project/pull/42")
    );
    assert!(result.pull_request_reused);
    assert!(result.merged);
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
