use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::designs::Design;
use crate::runner::Benchmark;
use crate::runner::BenchmarkResult;
use crate::util::{RunOptions, RunResult, run_binary};

#[derive(Debug, Clone)]
pub struct YosysSynthBenchmark {
    yosys_path: PathBuf,
    cell_lib: PathBuf,
    output_dir: PathBuf,
    output: RunOptions,
}

impl YosysSynthBenchmark {
    pub fn new(yosys_path: PathBuf, cell_lib: PathBuf, output_dir: PathBuf) -> Self {
        Self {
            yosys_path,
            cell_lib,
            output_dir,
            output: RunOptions::default(),
        }
    }

    pub fn yosys_path(&self) -> &PathBuf {
        &self.yosys_path
    }

    pub fn cell_lib(&self) -> &PathBuf {
        &self.cell_lib
    }

    pub fn with_output(mut self, output: RunOptions) -> Self {
        self.output = output;
        self
    }
}

impl Benchmark<YosysSynthResult> for YosysSynthBenchmark {
    fn run(&self, design: &impl Design) -> io::Result<YosysSynthResult> {
        fs::create_dir_all(&self.output_dir)?;
        let script_path = self.output_dir.join("script.ys");
        let stat_log_out = self.output_dir.join("synth.stat");
        let verilog_design = design.into_verilog();
        let commands = {
            let mut commands = vec![];

            for file in verilog_design.files() {
                commands.push(format!("read -sv \"{}\"", file.display()));
            }

            commands.push(format!("hierarchy -top {}", verilog_design.top()));
            commands.push(format!("synth -top {} -noabc", verilog_design.top()));
            commands.push(format!(
                "dfflibmap -liberty \"{}\"",
                self.cell_lib().display()
            ));
            commands.push(format!("abc -liberty \"{}\"", &self.cell_lib().display()));
            commands.push("clean".into());
            commands.push(format!(
                "tee -o \"{}\" stat -liberty \"{}\"",
                stat_log_out.display(),
                &self.cell_lib.display()
            ));
            commands
        };
        fs::write(&script_path, format!("{}\n", commands.join("\n")))?;
        let process = run_binary(
            self.yosys_path(),
            [OsStr::new("-s"), script_path.as_os_str()],
            &self.output,
        )?;
        Ok(YosysSynthResult { process })
    }
}

#[derive(Debug)]
pub struct YosysSynthResult {
    pub process: RunResult,
}

impl BenchmarkResult for YosysSynthResult {}
