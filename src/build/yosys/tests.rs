use std::{process::Output, time::UNIX_EPOCH};

use super::*;

struct Fixture {
    root: tempfile::TempDir,
    builder: YosysBuilder,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let repository = root.path().join("repository with spaces");
        fs::create_dir(&repository).unwrap();
        git_output(&repository, &["init", "-b", "main"]).unwrap();
        git_output(&repository, &["config", "user.name", "Test"]).unwrap();
        git_output(&repository, &["config", "user.email", "test@example.com"]).unwrap();
        let builder = YosysBuilder {
            repository,
            cache_dir: root.path().join("cache with spaces"),
            jobs: None,
        };
        Self { root, builder }
    }

    fn write(&self, path: &str, contents: &str) {
        fs::write(self.builder.repository.join(path), contents).unwrap();
    }

    fn cmake(&self, version: &str) {
        self.write("yosys", &format!("#!/bin/sh\nprintf '%s\\n' '{version}'\n"));
        self.write("yosys-abc", "#!/bin/sh\nexit 0\n");
        self.write("simlib.v", "// library\n");
        self.write("simcells.v", "// cells\n");
        self.write(
            "CMakeLists.txt",
            r#"
cmake_minimum_required(VERSION 3.16)
project(test_yosys NONE)
if(NOT CMAKE_BUILD_TYPE STREQUAL "RelWithDebInfo")
    message(FATAL_ERROR "Expected optimized build with debug information")
endif()
install(PROGRAMS yosys yosys-abc DESTINATION bin)
install(FILES simlib.v simcells.v DESTINATION share/yosys)
"#,
        );
    }

    fn commit(&self) -> Commit {
        git_output(&self.builder.repository, &["add", "."]).unwrap();
        git_output(&self.builder.repository, &["commit", "-m", "test build"]).unwrap();
        Commit {
            id: CommitId(
                git_output(&self.builder.repository, &["rev-parse", "HEAD"])
                    .unwrap()
                    .trim()
                    .into(),
            ),
            committed_at: UNIX_EPOCH,
            author: "Test".into(),
            commit_message: "test build".into(),
        }
    }

    fn logs(&self) -> String {
        fs::read_dir(self.builder.cache_dir.join("logs"))
            .unwrap()
            .map(|entry| fs::read_to_string(entry.unwrap().path()).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn execute(tool: &Yosys) -> Output {
    Command::new(&tool.executable).output().unwrap()
}

#[test]
fn builds_selected_commits_without_worktree_edits_and_reuses_cache() {
    let mut fixture = Fixture::new();
    fixture.cmake("first");
    let first = fixture.commit();
    fixture.cmake("second");
    let second = fixture.commit();
    fixture.write("yosys", "uncommitted edit");
    fixture.write("untracked", "keep me");
    let status = git_output(&fixture.builder.repository, &["status", "--porcelain"]).unwrap();
    let tool = fixture.builder.build(&first).unwrap();
    let jobs = std::thread::available_parallelism().unwrap();
    assert!(
        fixture
            .logs()
            .contains(&format!("\"--parallel\" \"{jobs}\""))
    );
    assert!(tool.executable.is_absolute());
    assert_eq!(execute(&tool).stdout, b"first\n");
    fixture.builder.jobs = NonZeroUsize::new(usize::MAX);
    let cached = fixture.builder.build(&first).unwrap();
    assert_eq!(cached.executable, tool.executable);
    assert_eq!(fixture.logs().matches("configure:").count(), 1);
    let next = fixture.builder.build(&second).unwrap();
    let logs = fixture.logs();
    let compile_commands: Vec<_> = logs
        .lines()
        .filter(|line| line.starts_with("compile:"))
        .collect();
    assert_eq!(compile_commands.len(), 2);
    assert!(
        compile_commands
            .iter()
            .all(|line| line.contains(&format!("\"--parallel\" \"{jobs}\"")))
    );
    assert_ne!(next.executable, tool.executable);
    assert_eq!(execute(&next).stdout, b"second\n");
    assert_eq!(
        git_output(&fixture.builder.repository, &["status", "--porcelain"]).unwrap(),
        status
    );
    assert_eq!(
        git_output(&fixture.builder.repository, &["rev-parse", "HEAD"])
            .unwrap()
            .trim(),
        second.id.0
    );
    assert!(
        fs::read_dir(&fixture.builder.cache_dir)
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".build-"))
    );
}

#[test]
fn incomplete_or_damaged_installations_are_rebuilt() {
    let mut fixture = Fixture::new();
    fixture.cmake("working");
    let commit = fixture.commit();
    let tool = fixture.builder.build(&commit).unwrap();
    fs::remove_file(tool.data_dir.join("simlib.v")).unwrap();
    fixture.builder.build(&commit).unwrap();
    assert!(tool.data_dir.join("simlib.v").is_file());
    fs::write(&tool.executable, "#!/bin/sh\nexit 1\n").unwrap();
    fixture.builder.build(&commit).unwrap();
    assert!(execute(&tool).status.success());
    fs::remove_file(
        tool.executable
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(".complete"),
    )
    .unwrap();
    fixture.builder.build(&commit).unwrap();
    assert_eq!(fixture.logs().matches("configure:").count(), 4);
}

#[test]
fn concurrent_builders_share_one_completed_installation() {
    let fixture = Fixture::new();
    fixture.cmake("working");
    let commit = fixture.commit();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let mut builder = YosysBuilder {
                repository: fixture.builder.repository.clone(),
                cache_dir: fixture.builder.cache_dir.clone(),
                jobs: fixture.builder.jobs,
            };
            let commit = commit.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                builder.build(&commit).unwrap()
            })
        })
        .collect();
    let tools: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    assert_eq!(tools[0].executable, tools[1].executable);
    assert_eq!(fixture.logs().matches("configure:").count(), 1);
}

#[test]
fn configure_failure_preserves_log_without_publishing_installation() {
    let mut fixture = Fixture::new();
    fixture.cmake("bad");
    fixture.write(
        "CMakeLists.txt",
        "cmake_minimum_required(VERSION 3.16)\nmessage(FATAL_ERROR \"deliberate failure\")\n",
    );
    let commit = fixture.commit();
    let error = fixture.builder.build(&commit).unwrap_err();
    let Error::YosysBuild { stage, log, .. } = error else {
        panic!("{error}")
    };
    assert_eq!(stage, "configure");
    assert!(
        fs::read_to_string(log)
            .unwrap()
            .contains("deliberate failure")
    );
    assert!(
        !fixture
            .builder
            .cache_dir
            .join(&commit.id.0)
            .join(".complete")
            .exists()
    );
    fixture.builder.build(&commit).unwrap_err();
    assert_eq!(fixture.logs().matches("configure:").count(), 2);
}

#[test]
fn failed_smoke_test_is_not_a_completed_build() {
    let mut fixture = Fixture::new();
    fixture.cmake("bad");
    fixture.write("yosys-abc", "#!/bin/sh\nexit 7\n");
    let commit = fixture.commit();
    assert!(matches!(
        fixture.builder.build(&commit),
        Err(Error::YosysBuild {
            stage: "ABC validation",
            ..
        })
    ));
    assert!(
        !fixture
            .builder
            .cache_dir
            .join(&commit.id.0)
            .join(".complete")
            .exists()
    );
}

#[test]
fn cache_cannot_be_shared_between_repositories_or_recipes() {
    let mut fixture = Fixture::new();
    fixture.cmake("working");
    let commit = fixture.commit();
    fixture.builder.build(&commit).unwrap();
    let other = fixture.root.path().join("other");
    git_output(
        fixture.root.path(),
        &[
            "clone",
            fixture.builder.repository.to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    )
    .unwrap();
    let mut other_builder = YosysBuilder {
        repository: other,
        cache_dir: fixture.builder.cache_dir.clone(),
        jobs: None,
    };
    assert!(matches!(
        other_builder.build(&commit),
        Err(Error::BuildCache(_))
    ));
    let path = fixture.builder.cache_dir.join("cache.json");
    let mut identity: CacheIdentity = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    identity.recipe_version += 1;
    fs::write(path, serde_json::to_vec(&identity).unwrap()).unwrap();
    assert!(matches!(
        fixture.builder.build(&commit),
        Err(Error::BuildCache(_))
    ));
}

#[test]
fn rejects_noncanonical_or_missing_revisions_before_creating_cache() {
    let mut fixture = Fixture::new();
    fixture.cmake("working");
    let mut commit = fixture.commit();
    for id in ["HEAD", "../../escape", &commit.id.0[..8], &"0".repeat(40)] {
        let mut invalid = commit.clone();
        invalid.id.0 = id.into();
        assert!(fixture.builder.build(&invalid).is_err());
    }
    commit.id.0 = commit.id.0.to_uppercase();
    assert!(fixture.builder.build(&commit).is_err());
    assert!(!fixture.builder.cache_dir.exists());
}

#[test]
fn legacy_make_build_installs_into_private_prefix() {
    let mut fixture = Fixture::new();
    fixture.builder.jobs = NonZeroUsize::new(1);
    fixture.cmake("legacy");
    fs::remove_file(fixture.builder.repository.join("CMakeLists.txt")).unwrap();
    fixture.write("Makefile", "all:\n\t@true\ninstall:\n\tmkdir -p \"$(PREFIX)/bin\" \"$(PREFIX)/share/yosys\"\n\tcp yosys yosys-abc \"$(PREFIX)/bin/\"\n\tchmod +x \"$(PREFIX)/bin/yosys\" \"$(PREFIX)/bin/yosys-abc\"\n\tcp simlib.v simcells.v \"$(PREFIX)/share/yosys/\"\n");
    let commit = fixture.commit();
    let tool = fixture.builder.build(&commit).unwrap();
    assert_eq!(execute(&tool).stdout, b"legacy\n");
    assert!(fixture.logs().contains("\"make\" \"-j\" \"1\""));
}

#[test]
fn local_submodules_use_the_committed_object_instead_of_dirty_sources() {
    let mut fixture = Fixture::new();
    let dependency = fixture.root.path().join("dependency");
    fs::create_dir(&dependency).unwrap();
    git_output(&dependency, &["init"]).unwrap();
    git_output(&dependency, &["config", "user.name", "Test"]).unwrap();
    git_output(&dependency, &["config", "user.email", "test@example.com"]).unwrap();
    fs::write(dependency.join("version"), "committed").unwrap();
    git_output(&dependency, &["add", "."]).unwrap();
    git_output(&dependency, &["commit", "-m", "dependency"]).unwrap();
    git_output(
        &fixture.builder.repository,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            dependency.to_str().unwrap(),
            "libs/dependency",
        ],
    )
    .unwrap();
    fixture.cmake("working");
    let cmake = fs::read_to_string(fixture.builder.repository.join("CMakeLists.txt")).unwrap();
    fixture.write("CMakeLists.txt", &(cmake + "\nfile(READ libs/dependency/version value)\nif(NOT value STREQUAL \"committed\")\nmessage(FATAL_ERROR \"wrong submodule\")\nendif()\n"));
    let commit = fixture.commit();
    fixture.write("libs/dependency/version", "dirty");
    fixture.builder.build(&commit).unwrap();
    assert_eq!(
        fs::read_to_string(fixture.builder.repository.join("libs/dependency/version")).unwrap(),
        "dirty"
    );
    assert!(fixture.logs().contains("submodule checkout:"));
}

/// Optional real build using installed toolchains and existing repository objects.
/// Defaults match run_mcm; override BENCHMARKS_YOSYS_{REPOSITORY,COMMIT,CACHE}.
#[test]
#[ignore = "compiles and caches a real Yosys installation"]
fn real_yosys_build() {
    use crate::history::{BranchHistory, RevisionSelection};
    let repository = std::env::var_os("BENCHMARKS_YOSYS_REPOSITORY")
        .map(PathBuf::from)
        .unwrap_or_else(|| "../yosys".into());
    let revision = std::env::var("BENCHMARKS_YOSYS_COMMIT").unwrap_or_else(|_| "HEAD".into());
    let commits = BranchHistory {
        repository: repository.clone(),
        branch: "main".into(),
    }
    .resolve(&RevisionSelection::Explicit(vec![CommitId(revision)]))
    .unwrap();
    let mut builder = YosysBuilder {
        repository,
        cache_dir: std::env::var_os("BENCHMARKS_YOSYS_CACHE")
            .map(PathBuf::from)
            .unwrap_or_else(|| "output/builds/yosys-relwithdebinfo".into()),
        jobs: None,
    };
    let tool = builder.build(&commits[0]).unwrap();
    let cached = builder.build(&commits[0]).unwrap();
    assert_eq!(tool.executable, cached.executable);
    let workspace = tempfile::tempdir().unwrap();
    fs::write(
        workspace.path().join("design.v"),
        "module top(input [7:0] a, b, output [8:0] y); assign y = a + b; endmodule\n",
    )
    .unwrap();
    let output = Command::new(&tool.executable)
        .current_dir(workspace.path())
        .args([
            "-Q",
            "-T",
            "-p",
            "read_verilog design.v; synth -top top; check -assert",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
