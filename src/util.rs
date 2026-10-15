use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub enum OutputTarget {
    #[default]
    Inherit,
    Capture,
    File(PathBuf),
}

impl OutputTarget {
    fn stdio(&self) -> io::Result<Stdio> {
        match self {
            Self::Inherit => Ok(Stdio::inherit()),
            Self::Capture => Ok(Stdio::piped()),
            Self::File(path) => Ok(File::create(path)?.into()),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    pub stdout: OutputTarget,
    pub stderr: OutputTarget,
}

#[derive(Debug)]
pub struct RunResult {
    pub status: ExitStatus,
    /// Wall-clock time from spawning the process until it finishes.
    pub elapsed: Duration,
    /// Present only for captured streams. Invalid UTF-8 is replaced lossily.
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

pub fn run_binary<I, S>(
    binary: impl AsRef<OsStr>,
    args: I,
    options: &RunOptions,
) -> io::Result<RunResult>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(binary);
    command
        .args(args)
        .stdout(options.stdout.stdio()?)
        .stderr(options.stderr.stdio()?);

    let start = Instant::now();
    let output = command.spawn()?.wait_with_output()?;
    let elapsed = start.elapsed();
    Ok(RunResult {
        status: output.status,
        elapsed,
        stdout: matches!(options.stdout, OutputTarget::Capture)
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned()),
        stderr: matches!(options.stderr, OutputTarget::Capture)
            .then(|| String::from_utf8_lossy(&output.stderr).into_owned()),
    })
}
