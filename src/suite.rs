use std::path::PathBuf;

use crate::designs::verilog::VerilogDesign;

pub trait SuiteResult {}

pub trait Suite<T>
where
    T: SuiteResult,
{
    fn run(&self, design: VerilogDesign) -> T;
}

#[derive(Debug, Clone)]
struct SynthSuite {
    yosys_path: PathBuf,
    cell_lib: PathBuf,
}

impl Suite<SynthResult> for SynthSuite {
    fn run(&self, design: VerilogDesign) -> SynthResult {
        todo!()
    }
}

struct SynthResult {}

impl SuiteResult for SynthResult {}
