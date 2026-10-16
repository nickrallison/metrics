use std::{
    path::PathBuf,
    process::{Command, Output},
};

use tempfile::TempDir;

use super::*;

struct Repository {
    directory: TempDir,
    root: CommitId,
    main: CommitId,
    side: CommitId,
    merge: CommitId,
    tip: CommitId,
}

fn command(directory: &std::path::Path) -> Command {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(directory)
        .args([
            "-c",
            "user.name=Test Author",
            "-c",
            "user.email=test@example.test",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .env("GIT_AUTHOR_NAME", "Ada Δ")
        .env("GIT_AUTHOR_EMAIL", "ada@example.test")
        .env("GIT_COMMITTER_NAME", "Test Committer")
        .env("GIT_COMMITTER_EMAIL", "committer@example.test");
    command
}

fn success(command: &mut Command) -> Output {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn git(directory: &std::path::Path, args: &[&str]) -> String {
    String::from_utf8(success(command(directory).args(args)).stdout).unwrap()
}

fn commit(
    directory: &std::path::Path,
    tree: &str,
    parents: &[&CommitId],
    seconds: u64,
    message: &str,
) -> CommitId {
    let mut command = command(directory);
    command.args(["commit-tree", tree, "-m", message]);
    for parent in parents {
        command.args(["-p", &parent.0]);
    }
    let date = format!("{seconds} +0000");
    command
        .env("GIT_AUTHOR_DATE", &date)
        .env("GIT_COMMITTER_DATE", date);
    CommitId(
        String::from_utf8(success(&mut command).stdout)
            .unwrap()
            .trim()
            .into(),
    )
}

impl Repository {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path();
        git(
            path,
            &["init", "--quiet", "--initial-branch=main", "--template="],
        );
        let tree = git(path, &["mktree"]);
        let tree = tree.trim();
        // Deliberately skew timestamps: traversal must follow ancestry, not dates.
        let root = commit(path, tree, &[], 1_700_000_300, "Root\n");
        let main = commit(path, tree, &[&root], 1_700_000_100, "Main\n");
        let side = commit(path, tree, &[&root], 1_700_000_500, "Side\n");
        let merge = commit(path, tree, &[&main, &side], 1_700_000_000, "Merge\n");
        let tip = commit(
            path,
            tree,
            &[&merge],
            1_700_000_200,
            "Tip subject\n\nBody with café and tabs\tkept.\n",
        );
        git(path, &["update-ref", "refs/heads/main", &tip.0]);
        git(path, &["update-ref", "refs/heads/feature", &side.0]);
        Self {
            directory,
            root,
            main,
            side,
            merge,
            tip,
        }
    }

    fn history(&self) -> BranchHistory {
        BranchHistory {
            repository: self.directory.path().into(),
            branch: "main".into(),
        }
    }
}

fn ids(commits: &[Commit]) -> Vec<CommitId> {
    commits.iter().map(|commit| commit.id.clone()).collect()
}

#[test]
fn since_excludes_anchor_and_side_commits_and_preserves_ancestry_order() {
    let repo = Repository::new();
    let commits = repo
        .history()
        .resolve(&RevisionSelection::Since(repo.root.clone()))
        .unwrap();
    assert_eq!(ids(&commits), vec![repo.main, repo.merge, repo.tip]);
    assert_eq!(
        commits[2].committed_at,
        UNIX_EPOCH + Duration::from_secs(1_700_000_200)
    );
    assert_eq!(commits[2].author, "Ada Δ <ada@example.test>");
    assert_eq!(
        commits[2].commit_message,
        "Tip subject\n\nBody with café and tabs\tkept.\n"
    );
}

#[test]
fn explicit_preserves_input_order_and_expands_abbreviated_ids() {
    let repo = Repository::new();
    let commits = repo
        .history()
        .resolve(&RevisionSelection::Explicit(vec![
            CommitId(repo.tip.0[..12].into()),
            repo.root.clone(),
            repo.main.clone(),
        ]))
        .unwrap();
    assert_eq!(ids(&commits), vec![repo.tip, repo.root, repo.main]);
}

#[test]
fn merged_side_commit_is_rejected_for_both_selection_modes() {
    let repo = Repository::new();
    for selection in [
        RevisionSelection::Since(repo.side.clone()),
        RevisionSelection::Explicit(vec![repo.side.clone()]),
    ] {
        let error = repo.history().resolve(&selection).unwrap_err();
        assert!(matches!(error, Error::InvalidRevision(_)));
        assert!(error.to_string().contains("first-parent history"));
    }
}

#[test]
fn duplicate_ids_are_rejected_after_normalizing_abbreviations() {
    let repo = Repository::new();
    let selection =
        RevisionSelection::Explicit(vec![repo.tip.clone(), CommitId(repo.tip.0[..12].into())]);
    let error = repo.history().resolve(&selection).unwrap_err();
    assert!(matches!(error, Error::InvalidRevision(_)));
    assert!(error.to_string().contains("duplicate commit"));
}

#[test]
fn tip_anchor_and_empty_explicit_selection_return_no_commits() {
    let repo = Repository::new();
    for selection in [
        RevisionSelection::Since(repo.tip.clone()),
        RevisionSelection::Explicit(vec![]),
    ] {
        assert!(repo.history().resolve(&selection).unwrap().is_empty());
    }
}

#[test]
fn selected_branch_is_independent_of_head_and_same_named_tags() {
    let repo = Repository::new();
    let path = repo.directory.path();
    git(path, &["symbolic-ref", "HEAD", "refs/heads/feature"]);
    git(path, &["update-ref", "refs/tags/main", &repo.side.0]);
    std::fs::write(path.join("untracked"), "keep me").unwrap();
    let status = git(path, &["status", "--porcelain"]);
    let commits = repo
        .history()
        .resolve(&RevisionSelection::Since(repo.main.clone()))
        .unwrap();
    assert_eq!(ids(&commits), vec![repo.merge, repo.tip]);
    assert_eq!(
        git(path, &["symbolic-ref", "HEAD"]).trim(),
        "refs/heads/feature"
    );
    assert_eq!(git(path, &["status", "--porcelain"]), status);
    assert_eq!(
        std::fs::read_to_string(path.join("untracked")).unwrap(),
        "keep me"
    );
}

#[test]
fn fully_qualified_branch_and_remote_tracking_refs_are_supported() {
    let repo = Repository::new();
    git(
        repo.directory.path(),
        &["update-ref", "refs/remotes/origin/main", &repo.tip.0],
    );
    for branch in ["refs/heads/main", "refs/remotes/origin/main"] {
        let mut history = repo.history();
        history.branch = branch.into();
        let commits = history
            .resolve(&RevisionSelection::Since(repo.merge.clone()))
            .unwrap();
        assert_eq!(ids(&commits), vec![repo.tip.clone()]);
    }
}

#[test]
fn missing_branches_and_option_like_revisions_report_git_errors() {
    let repo = Repository::new();
    let mut missing = repo.history();
    missing.branch = "missing".into();
    assert!(matches!(
        missing.resolve(&RevisionSelection::Explicit(vec![])),
        Err(Error::Git { .. })
    ));
    for revision in ["missing-revision", "--all"] {
        let error = repo
            .history()
            .resolve(&RevisionSelection::Since(CommitId(revision.into())))
            .unwrap_err();
        match error {
            Error::Git {
                repository,
                status,
                stderr,
                ..
            } => {
                assert_eq!(repository, repo.directory.path());
                assert!(!status.success());
                assert!(!stderr.is_empty());
            }
            error => panic!("unexpected error: {error}"),
        }
    }
}

#[test]
fn non_repository_paths_report_errors() {
    let directory = tempfile::tempdir().unwrap();
    let history = BranchHistory {
        repository: PathBuf::from(directory.path()),
        branch: "main".into(),
    };
    assert!(matches!(
        history.resolve(&RevisionSelection::Explicit(vec![])),
        Err(Error::Git { .. })
    ));
}

#[test]
fn log_parser_supports_sha256_and_dates_before_epoch() {
    let id = "a".repeat(64);
    let output = format!("{id}\0-1\0Author\0Subject\n\nBody\n\0");
    let commits = parse_log(output.as_bytes()).unwrap();
    assert_eq!(commits[0].id.0, id);
    assert_eq!(commits[0].committed_at, UNIX_EPOCH - Duration::from_secs(1));
    assert_eq!(commits[0].commit_message, "Subject\n\nBody\n");
}

#[test]
fn malformed_log_output_is_rejected_without_panicking() {
    let invalid_time = format!("{}\0not-a-time\0author\0message\0", "a".repeat(40));
    for output in [
        b"".as_slice(),
        b"incomplete\0",
        b"not terminated",
        b"\xff\0",
        invalid_time.as_bytes(),
    ] {
        assert!(matches!(parse_log(output), Err(Error::InvalidGitOutput(_))));
    }
}
