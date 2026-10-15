use std::path::PathBuf;

use crate::designs::Design;
use crate::runner::Benchmark;
use crate::runner::BenchmarkResult;

#[derive(Debug, Clone)]
pub struct YosysSynthBenchmark {
    yosys_path: PathBuf,
    cell_lib: PathBuf,
    stat_log_out: PathBuf,
    // commands: Vec<String>,
}

impl YosysSynthBenchmark {
    pub fn new(yosys_path: PathBuf, cell_lib: PathBuf, commands: Vec<String>) -> Self {
        Self {
            yosys_path,
            cell_lib,
            // commands,
        }
    }

    pub fn yosys_path(&self) -> &PathBuf {
        &self.yosys_path
    }

    pub fn cell_lib(&self) -> &PathBuf {
        &self.cell_lib
    }

    // pub fn commands(&self) -> &[String] {
    //     &self.commands
    // }
}

impl Benchmark<YosysSynthResult> for YosysSynthBenchmark {
    fn run(&self, design: &impl Design) -> YosysSynthResult {
        let verilog_design = design.into_verilog();
        let commands = {
            let mut commands = vec![];

            for file in verilog_design.files() {
                commands.push(format!("read -sv \"{}\"", file.display()));
            }

            commands.push(format!("hierarchy -top {}", verilog_design.top()));
            commands.push(format!("synth -top {} -noabc", verilog_design.top()));
            commands.push(format!("abc -liberty \"{}\"", &self.cell_lib().display()));
            commands.push(format!(
                "tee -o \"{}\" stat -liberty \"{}\"",
                &self.stat_log_out.display(),
                &self.cell_lib.display()
            ));
            commands
        };
    }
}

pub struct YosysSynthResult {}

impl BenchmarkResult for YosysSynthResult {}
