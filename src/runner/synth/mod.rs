use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::designs::Design;
use crate::runner::Benchmark;
use crate::runner::BenchmarkResult;
use crate::util::{RunOptions, RunResult, run_binary};

mod commands;

use commands::Commands;

#[derive(Debug, Clone)]
pub struct SynthBenchmark {
    name: String,
    cell_lib: PathBuf,
    // input_commands: Commands,
    synth_commands: Commands,
    output_commands: Commands,
}

impl SynthBenchmark {
    pub fn new(name: String, cell_lib: PathBuf) -> Self {
        Self {
            name,
            cell_lib,
            synth_commands: Commands::default_synth(),
            output_commands: Commands::default(),
        }
    }

    pub fn cell_lib(&self) -> &PathBuf {
        &self.cell_lib
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

        let run_options = RunOptions::default();

        let process = run_binary(
            self.yosys_path(),
            [OsStr::new("-s"), script_path.as_os_str()],
            &run_options,
        )?;
        Ok(YosysSynthResult { process })
    }
}

#[derive(Debug)]
pub struct YosysSynthResult {
    pub process: RunResult,
}

impl BenchmarkResult for YosysSynthResult {}
