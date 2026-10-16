use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io::Write,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde::{Deserialize, Serialize};

use super::Builder;
use crate::{
    Error, Result,
    history::{Commit, CommitId},
};

// Bump when changing compilation options or installed contents. Compiler caching
// and job limits do not invalidate already completed installations.
const RECIPE_VERSION: u32 = 2;

#[derive(Debug, Clone)]
pub struct Yosys {
    pub commit: CommitId,
    pub executable: PathBuf,
    pub data_dir: PathBuf,
}

/// Builds committed sources, ignoring worktree edits, into a private installation.
///
/// Uses CMake RelWithDebInfo when available, otherwise the historical Make build. CMake
/// uses bundled dependencies with unit tests disabled; both builds include ABC.
/// Compilation uses the available CPU count; `jobs` can lower that limit.
/// Fresh builds require ccache and share compiler output across commits.
/// Git may fetch missing submodules.
///
/// One cache directory belongs to one canonical repository path and build recipe.
/// Builders sharing a cache serialize access with an operating-system file lock.
#[derive(Debug)]
pub struct YosysBuilder {
    pub repository: PathBuf,
    pub cache_dir: PathBuf,
    /// Maximum compiler jobs, capped at the available CPU count. `None` uses all.
    pub jobs: Option<NonZeroUsize>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct CacheIdentity {
    repository: PathBuf,
    recipe_version: u32,
}

impl Builder for YosysBuilder {
    type Tool = Yosys;

    fn build(&mut self, commit: &Commit) -> Result<Yosys> {
        // IDs become directory names and checkout arguments, so accept only the
        // full immutable object names supplied by history resolution.
        let id = &commit.id.0;
        if !matches!(id.len(), 40 | 64) || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Error::InvalidRevision(format!(
                "Yosys builds require a full commit ID: {id}"
            )));
        }
        let repository = self.repository.canonicalize()?;
        let resolved = git_output(
            &repository,
            &["rev-parse", "--verify", &format!("{id}^{{commit}}")],
        )?;
        if resolved.trim() != id {
            return Err(Error::InvalidRevision(format!(
                "not a canonical commit ID: {id}"
            )));
        }
        fs::create_dir_all(&self.cache_dir)?;
        let cache = self.cache_dir.canonicalize()?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(cache.join(".lock"))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                eprintln!("Waiting for Yosys build cache: {}", cache.display());
                lock.lock()?;
            }
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
        check_cache_identity(&cache, repository.clone())?;
        let prefix = cache.join(id);
        let tool = Yosys {
            commit: commit.id.clone(),
            executable: prefix.join("bin/yosys"),
            data_dir: prefix.join("share/yosys"),
        };
        let mut log = BuildLog::new(&cache, id)?;
        let marker = prefix.join(".complete");
        if fs::read_to_string(&marker).is_ok_and(|value| value == *id) {
            if validate(&tool, &mut log).is_ok() {
                eprintln!("Using cached Yosys {id}");
                return Ok(tool);
            }
            writeln!(
                log.file,
                "Cached installation failed validation; rebuilding."
            )?;
        }
        log.run("ccache", Command::new("ccache").arg("--version"))?;
        if prefix.exists() {
            fs::remove_dir_all(&prefix)?;
        }
        fs::create_dir(&prefix)?;
        let workspace = tempfile::Builder::new()
            .prefix(".build-")
            .tempdir_in(&cache)?;
        let source = workspace.path().join("source");
        let build = workspace.path().join("build");
        let compiler_cache = cache.parent().unwrap_or(&cache).join("ccache");
        let compiler_tmp = workspace.path().join("ccache-tmp");
        fs::create_dir_all(&compiler_cache)?;
        fs::create_dir(&compiler_tmp)?;
        // Normalize paths relative to each workspace, not the shared cache root:
        // the random .build-* component must not become part of compiler keys.
        let compiler_environment = [
            ("CCACHE_DIR", compiler_cache.as_os_str()),
            ("CCACHE_BASEDIR", workspace.path().as_os_str()),
            ("CCACHE_TEMPDIR", compiler_tmp.as_os_str()),
        ];
        log.run(
            "ccache configuration",
            Command::new("ccache")
                .args(["--max-size", "20GiB"])
                .envs(compiler_environment),
        )?;
        writeln!(log.file, "Compiler cache: {}", compiler_cache.display())?;
        let available = std::thread::available_parallelism()?;
        let jobs = self.jobs.unwrap_or(available).min(available).to_string();
        eprintln!(
            "Building Yosys {id} with {jobs} jobs; log: {}",
            log.path.display()
        );
        log.run(
            "clone",
            Command::new("git")
                .args(["clone", "--shared", "--no-checkout", "--"])
                .arg(&repository)
                .arg(&source),
        )?;
        log.run(
            "checkout",
            Command::new("git")
                .arg("-C")
                .arg(&source)
                .args(["checkout", "--detach", id]),
        )?;
        seed_submodules(&repository, &source, &mut log)?;
        log.run(
            "submodules",
            Command::new("git").arg("-C").arg(&source).args([
                "submodule",
                "update",
                "--init",
                "--checkout",
            ]),
        )?;
        if source.join("CMakeLists.txt").is_file() {
            log.run(
                "configure",
                Command::new("cmake")
                    .arg("-S")
                    .arg(&source)
                    .arg("-B")
                    .arg(&build)
                    .arg("-DCMAKE_BUILD_TYPE=RelWithDebInfo")
                    .arg(format!("-DCMAKE_INSTALL_PREFIX={}", prefix.display()))
                    .arg("-DCMAKE_INSTALL_BINDIR=bin")
                    .arg("-DCMAKE_INSTALL_DATADIR=share")
                    .arg("-DYOSYS_USE_BUNDLED_LIBS=ON")
                    .arg("-DYOSYS_ENABLE_UNIT_TESTS=OFF")
                    .arg("-DYOSYS_COMPILER_LAUNCHER=ccache")
                    .arg("-DCMAKE_C_COMPILER_LAUNCHER=ccache")
                    .arg("-DCMAKE_CXX_COMPILER_LAUNCHER=ccache")
                    .envs(compiler_environment),
            )?;
            log.run(
                "compile",
                Command::new("cmake")
                    .arg("--build")
                    .arg(&build)
                    .args(["--config", "RelWithDebInfo", "--parallel", &jobs])
                    .envs(compiler_environment),
            )?;
            log.run(
                "install",
                Command::new("cmake")
                    .arg("--install")
                    .arg(&build)
                    .args(["--config", "RelWithDebInfo"])
                    .envs(compiler_environment),
            )?;
        } else if source.join("Makefile").is_file() {
            // Command-line PREFIX takes precedence over historical defaults.
            let prefix_arg = format!("PREFIX={}", prefix.display());
            log.run(
                "compile",
                Command::new("make")
                    .current_dir(&source)
                    .args(["-j", &jobs, &prefix_arg, "ENABLE_CCACHE=1", "STRIP=true"])
                    .envs(compiler_environment),
            )?;
            log.run(
                "install",
                Command::new("make")
                    .current_dir(&source)
                    .args(["install", &prefix_arg, "ENABLE_CCACHE=1", "STRIP=true"])
                    .envs(compiler_environment),
            )?;
        } else {
            return Err(log.error("configure", "neither CMakeLists.txt nor Makefile exists"));
        }
        log.run(
            "ccache statistics",
            Command::new("ccache")
                .arg("--show-stats")
                .envs(compiler_environment),
        )?;
        validate(&tool, &mut log)?;
        // Publish readiness last. A failure or interruption is never a cache hit.
        let mut ready = tempfile::NamedTempFile::new_in(&prefix)?;
        ready.write_all(id.as_bytes())?;
        ready.persist(&marker).map_err(|error| error.error)?;
        Ok(tool)
    }
}

fn check_cache_identity(cache: &Path, repository: PathBuf) -> Result<()> {
    let expected = CacheIdentity {
        repository,
        recipe_version: RECIPE_VERSION,
    };
    let path = cache.join("cache.json");
    match fs::read(&path) {
        Ok(bytes) => {
            let actual: CacheIdentity = serde_json::from_slice(&bytes)?;
            if actual != expected {
                return Err(Error::BuildCache(format!(
                    "{} belongs to another repository or build recipe; choose a new cache directory",
                    cache.display()
                )));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut file = tempfile::NamedTempFile::new_in(cache)?;
            serde_json::to_writer_pretty(&mut file, &expected)?;
            file.persist(path).map_err(|error| error.error)?;
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

// Reuse local object databases where the pinned submodule commit is available.
// These temporary shared clones only borrow objects during the build; no source
// worktree edits are copied, and no Git command writes to the original checkout.
// Yosys requires its top-level submodules; standalone submodule test dependencies
// are deliberately not initialized recursively.
fn seed_submodules(repository: &Path, source: &Path, log: &mut BuildLog) -> Result<()> {
    let entries = git_output(source, &["ls-files", "--stage", "-z"])?;
    for entry in entries
        .split('\0')
        .filter(|entry| entry.starts_with("160000 "))
    {
        let (metadata, path) = entry
            .split_once('\t')
            .ok_or_else(|| Error::InvalidGitOutput("invalid gitlink entry".into()))?;
        let id = metadata
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| Error::InvalidGitOutput("missing gitlink ID".into()))?;
        let local = repository.join(path);
        if !local.join(".git").exists() {
            continue;
        }
        let available = Command::new("git")
            .arg("-C")
            .arg(&local)
            .args(["cat-file", "-e", &format!("{id}^{{commit}}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if available.success() {
            let destination = source.join(path);
            fs::create_dir_all(destination.parent().expect("gitlink has a parent"))?;
            log.run(
                "submodule clone",
                Command::new("git")
                    .args(["clone", "--shared", "--no-checkout", "--"])
                    .arg(&local)
                    .arg(&destination),
            )?;
            log.run(
                "submodule checkout",
                Command::new("git")
                    .arg("-C")
                    .arg(destination)
                    .args(["checkout", "--detach", id]),
            )?;
        }
    }
    Ok(())
}

fn git_output(repository: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(Error::Git {
            repository: repository.into(),
            command: args.join(" "),
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).trim().into(),
        });
    }
    String::from_utf8(output.stdout).map_err(|error| Error::InvalidGitOutput(error.to_string()))
}

fn validate(tool: &Yosys, log: &mut BuildLog) -> Result<()> {
    for name in ["simlib.v", "simcells.v"] {
        if !tool.data_dir.join(name).is_file() {
            return Err(log.error("validation", format!("missing data file {name}")));
        }
    }
    log.run(
        "validation",
        Command::new(&tool.executable).args([
            "-Q",
            "-T",
            "-q",
            "-p",
            "read_verilog -lib -D SIMLIB_NOCHECKS +/simlib.v; read_verilog -lib +/simcells.v",
        ]),
    )?;
    log.run(
        "ABC validation",
        Command::new(tool.executable.with_file_name("yosys-abc")).args(["-c", "quit"]),
    )
}

struct BuildLog {
    file: File,
    path: PathBuf,
    commit: String,
}

impl BuildLog {
    fn new(cache: &Path, commit: &str) -> Result<Self> {
        let directory = cache.join("logs");
        fs::create_dir_all(&directory)?;
        let (file, path) = tempfile::Builder::new()
            .prefix(&format!("{commit}-"))
            .suffix(".log")
            .tempfile_in(directory)?
            .keep()
            .map_err(|error| error.error)?;
        Ok(Self {
            file,
            path,
            commit: commit.into(),
        })
    }

    fn run(&mut self, stage: &'static str, command: &mut Command) -> Result<()> {
        writeln!(self.file, "\n{stage}: {command:?}")?;
        self.file.flush()?;
        let status = command
            .stdin(Stdio::null())
            .stdout(self.file.try_clone()?)
            .stderr(self.file.try_clone()?)
            .status()
            .map_err(|error| self.error(stage, error))?;
        if !status.success() {
            return Err(self.error(stage, status));
        }
        Ok(())
    }

    fn error(&self, stage: &'static str, detail: impl std::fmt::Display) -> Error {
        Error::YosysBuild {
            commit: self.commit.clone(),
            stage,
            detail: detail.to_string(),
            log: self.path.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
