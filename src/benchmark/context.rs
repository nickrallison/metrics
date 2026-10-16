use std::{
    fs,
    path::PathBuf,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    Error, Result,
    store::{Reference, ReferenceCache, ReferenceId},
};

/// Raw outputs are discarded unless explicitly requested; all flags default off.
///
/// Parsed statistics, timing samples, definitions, and outcome summaries are
/// always retained. Temporary files needed by another phase can exist without
/// becoming retained artifacts. Capturing stdout for parsing is not log retention.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ArtifactPolicy {
    pub logs: bool,
    pub scripts: bool,
    pub netlists: bool,
}

#[derive(Debug)]
pub struct ProcessOutput {
    pub output: Output,
    pub elapsed: Duration,
}

/// Isolated workspace and process/cache helpers for one comparison attempt.
///
/// Use fresh temporary paths for repeated invocations and remove temporary
/// parser inputs after use. Retained artifacts need distinct attempt paths.
/// The permanent directory convention is still an open implementation decision.
pub struct RunContext<'a, C: ReferenceCache> {
    pub workspace: PathBuf,
    pub artifacts: ArtifactPolicy,
    /// Only reuse reference timings within this compatible measurement session.
    /// `None` forces fresh reference measurements. Experiments set their own ID.
    pub reference_scope: Option<String>,
    cache: &'a mut C,
    processes: usize,
}

impl<'a, C: ReferenceCache> RunContext<'a, C> {
    pub fn new(workspace: PathBuf, artifacts: ArtifactPolicy, cache: &'a mut C) -> Self {
        Self {
            workspace,
            artifacts,
            reference_scope: None,
            cache,
            processes: 0,
        }
    }

    /// Measure immediately before spawning through exit/output collection.
    /// Preparation, parsing, and later artifact copying are outside this interval.
    /// Preserve nonzero termination and useful error context instead of treating
    /// captured stdout as proof of success. Timeout/cancellation support is pending.
    pub fn run_process(&mut self, command: &mut Command) -> Result<ProcessOutput> {
        if command.get_current_dir().is_none() {
            command.current_dir(&self.workspace);
        }
        command.stdin(Stdio::null());
        let description = format!("{command:?}");
        let index = self.processes;
        self.processes += 1;
        let start = Instant::now();
        let output = command.output()?;
        let elapsed = start.elapsed();
        if self.artifacts.logs {
            fs::write(self.workspace.join(format!("process-{index:03}.stdout.log")), &output.stdout)?;
            fs::write(self.workspace.join(format!("process-{index:03}.stderr.log")), &output.stderr)?;
        }
        if !output.status.success() {
            // Yosys errors normally go to stderr; other tools may use stdout.
            let detail = if output.stderr.is_empty() { &output.stdout } else { &output.stderr };
            let tail = &detail[detail.len().saturating_sub(8192)..];
            return Err(Error::Process {
                command: description, status: output.status,
                detail: String::from_utf8_lossy(tail).trim().into(),
            });
        }
        Ok(ProcessOutput { output, elapsed })
    }

    pub fn reference<T: DeserializeOwned>(&self, key: &str) -> Result<Option<Reference<T>>> {
        self.cache.get(key)
    }

    pub fn cache_reference<T: Serialize>(&mut self, key: &str, value: &T) -> Result<ReferenceId> {
        self.cache.insert(key, value)
    }
}
