use std::path::PathBuf;

use crate::designs::verilog::VerilogDesign;

pub trait BenchmarkResult {}

pub trait Benchmark<T>
where
    T: BenchmarkResult,
{
    fn run(&self, design: VerilogDesign) -> T;
}

#[derive(Debug, Clone)]
struct YosysSynthBenchmark {
    yosys_path: PathBuf,
    cell_lib: PathBuf,
}

impl Benchmark<YosysSynthResult> for YosysSynthBenchmark {
    fn run(&self, design: VerilogDesign) -> YosysSynthResult {
        todo!()
    }
}

struct YosysSynthResult {}

impl BenchmarkResult for YosysSynthResult {}
