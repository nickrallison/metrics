pub mod yosys_synth;

use std::io;

use crate::designs::Design;

pub trait Benchmark<T>
where
    T: BenchmarkResult,
{
    fn run(&self, design: &impl Design) -> io::Result<T>;
}

pub trait BenchmarkResult {}
