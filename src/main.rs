use std::path::PathBuf;

use crate::designs::verilog::VerilogDesign;
use crate::runner::yosys_synth::YosysSynthBenchmark;

mod designs;
mod runner;

fn main() {
    let design = VerilogDesign::new(
        "ExampleBoomSystem".into(),
        vec![PathBuf::from("verilog/MediumBoom.v")],
    );

    let runner = YosysSynthBenchmark::new(
        PathBuf::from("/home/nick/Projects/yosys/build-rel/yosys"),
        PathBuf::from(
            "/home/nick/Projects/yosys/tests/techmap/mcm/NangateOpenCellLibrary_typical.lib",
        ),
        // read_verilog "tests/techmap/mcm/results/test_0/unoptimized.v"
        // hierarchy -top unoptimized
        // synth -top unoptimized -noabc
        // abc -liberty "tests/techmap/mcm/NangateOpenCellLibrary_typical.lib"
        // tee -o "tests/techmap/mcm/results/test_0/synth.stat" stat -liberty "tests/techmap/mcm/NangateOpenCellLibrary_typical.lib"
        vec![],
    );

    println!("Hello, world!");
}
