use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde::Serialize;

use crate::{Error, Result};

/// Tools for diagnostic recordings, kept separate from benchmark timing samples.
#[derive(Debug, Serialize)]
pub struct PerfProfiler {
    perf: PathBuf,
    perl: PathBuf,
    addr2line: PathBuf,
    stackcollapse: PathBuf,
    flamegraph: PathBuf,
}

impl PerfProfiler {
    /// Resolve and check prerequisites before building or starting an experiment.
    pub fn new(addr2line: impl AsRef<Path>) -> Result<Self> {
        if !cfg!(target_os = "linux") {
            return Err(Error::Profiling("perf recording requires Linux".into()));
        }
        let tools = Self {
            perf: resolve(Path::new("perf"))?,
            perl: resolve(Path::new("perl"))?,
            addr2line: resolve(addr2line.as_ref())?,
            stackcollapse: resolve(Path::new("stackcollapse-perf.pl"))?,
            flamegraph: resolve(Path::new("flamegraph.pl"))?,
        };
        for command in [
            Command::new(&tools.perf).arg("--version"),
            Command::new(&tools.addr2line).arg("--version"),
            Command::new(&tools.perl)
                .arg("-c")
                .arg(&tools.stackcollapse),
            Command::new(&tools.perl).arg("-c").arg(&tools.flamegraph),
        ] {
            let output = command
                .output()
                .map_err(|error| Error::Profiling(format!("cannot run {command:?}: {error}")))?;
            if !output.status.success() {
                return Err(Error::Profiling(format!(
                    "prerequisite {command:?} failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                )));
            }
        }
        Ok(tools)
    }

    /// Record one extra invocation, then render source-annotated stacks to SVG.
    /// All pipeline stages must succeed; raw data and diagnostics survive failure.
    pub fn record(&self, command: &Command, directory: &Path) -> Result<()> {
        fs::create_dir_all(directory)?;
        let directory = directory.canonicalize()?;
        let data = directory.join("perf.data");
        let scratch = tempfile::tempdir_in(&directory)?;
        let script = scratch.path().join("perf.script");
        let folded = scratch.path().join("stacks.folded");
        let mut record = Command::new(&self.perf);
        record
            .args(["record", "-q", "-e", "cpu-clock:u", "-F", "999"])
            .args(["--call-graph", "dwarf,16384", "-o"])
            .arg(&data)
            .arg("--")
            .arg(command.get_program())
            .args(command.get_args())
            .current_dir(command.get_current_dir().unwrap_or(&directory))
            .stdin(Stdio::null());
        for (key, value) in command.get_envs() {
            match value {
                Some(value) => {
                    record.env(key, value);
                }
                None => {
                    record.env_remove(key);
                }
            }
        }
        stage(
            &mut record,
            &directory,
            "record",
            &directory.join("yosys.stdout.log"),
        )?;
        stage(
            Command::new(&self.perf)
                .args(["script", "-i"])
                .arg(&data)
                .arg(format!("--addr2line={}", self.addr2line.display()))
                .args(["--no-inline", "-F", "+srcline"])
                .env("DEBUGINFOD_URLS", ""),
            &directory,
            "script",
            &script,
        )?;
        stage(
            Command::new(&self.perl)
                .arg(&self.stackcollapse)
                .arg("--srcline")
                .stdin(File::open(&script)?),
            &directory,
            "collapse",
            &folded,
        )?;
        if fs::metadata(&folded)?.len() == 0 {
            return Err(Error::Profiling(format!(
                "no sampled stacks in {}; the process may be too short to sample",
                data.display()
            )));
        }
        stage(
            Command::new(&self.perl)
                .arg(&self.flamegraph)
                .stdin(File::open(&folded)?),
            &directory,
            "flamegraph",
            &directory.join("flamegraph.svg"),
        )
    }
}

fn stage(command: &mut Command, directory: &Path, name: &str, stdout: &Path) -> Result<()> {
    writeln!(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join("commands.log"))?,
        "{name}: {command:?}"
    )?;
    command.stdout(File::create(stdout)?);
    let output = command
        .output()
        .map_err(|error| Error::Profiling(format!("cannot run {command:?}: {error}")))?;
    fs::write(directory.join(format!("{name}.stderr.log")), &output.stderr)?;
    if !output.status.success() {
        let tail = &output.stderr[output.stderr.len().saturating_sub(8192)..];
        return Err(Error::Profiling(format!(
            "{name} failed ({}): {}; artifacts: {}",
            output.status,
            String::from_utf8_lossy(tail).trim(),
            directory.display()
        )));
    }
    Ok(())
}

fn resolve(path: &Path) -> Result<PathBuf> {
    if path.components().count() == 1
        && !path.is_absolute()
        && let Some(paths) = std::env::var_os("PATH")
    {
        for directory in std::env::split_paths(&paths) {
            let candidate = directory.join(path);
            if candidate.is_file() {
                return Ok(std::path::absolute(candidate)?);
            }
        }
    }
    if path.is_file() {
        // llvm-addr2line is often a symlink to llvm-symbolizer. Its invoked name
        // selects the protocol, so resolving the final symlink can hang perf.
        return Ok(std::path::absolute(path)?);
    }
    Err(Error::Profiling(format!(
        "cannot find prerequisite {}; see docs/profiling.md",
        path.display()
    )))
}

#[cfg(test)]
mod tests;
