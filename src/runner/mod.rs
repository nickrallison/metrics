pub mod yosys_synth;

use std::path::PathBuf;

use crate::designs::Design;

pub trait Benchmark<T>
where
    T: BenchmarkResult,
{
    fn run(&self, design: &impl Design) -> T;
}

pub trait BenchmarkResult {}

#[derive(Debug, Clone)]
struct YosysSynthBenchmark {
    yosys_path: PathBuf,
    cell_lib: PathBuf,
}

impl Benchmark<YosysSynthResult> for YosysSynthBenchmark {
    fn run(&self, design: &impl Design) -> YosysSynthResult {
        todo!()
    }
}

struct YosysSynthResult {}

impl BenchmarkResult for YosysSynthResult {}
