pub mod synth;

use std::io;
use std::path::Path;

use crate::designs::Design;
use crate::yosys::Yosys;

pub trait Benchmark<T>
where
    T: BenchmarkResult,
{
    fn run(&self, design: &impl Design, yosys: &Path, output_dir: &Path) -> io::Result<T>;
}

pub trait BenchmarkResult {}
