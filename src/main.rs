use std::path::PathBuf;

use crate::designs::verilog::VerilogDesign;
use crate::runner::Benchmark;
use crate::runner::yosys_synth::YosysSynthBenchmark;
use crate::util::{OutputTarget, RunOptions};

mod designs;
mod runner;
pub mod util;

fn main() -> std::io::Result<()> {
    let design = VerilogDesign::new(
        "ExampleBoomSystem".into(),
        vec![PathBuf::from("verilog/MediumBoom.v")],
    );

    let output_dir = PathBuf::from("output");
    let runner = YosysSynthBenchmark::new(
        PathBuf::from("/home/nick/Projects/yosys/build-rel/yosys"),
        PathBuf::from(
            "/home/nick/Projects/yosys/tests/techmap/mcm/NangateOpenCellLibrary_typical.lib",
        ),
        output_dir.clone(),
    )
    .with_output(RunOptions {
        stdout: OutputTarget::File(output_dir.join("stdout.log")),
        stderr: OutputTarget::File(output_dir.join("stderr.log")),
    });

    let result = runner.run(&design)?;
    eprintln!(
        "Yosys: {}, elapsed: {:.2?}",
        result.process.status, result.process.elapsed
    );
    if !result.process.status.success() {
        return Err(std::io::Error::other(format!(
            "Yosys failed with {}; see output/stdout.log and output/stderr.log",
            result.process.status
        )));
    }
    Ok(())
}
