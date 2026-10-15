//! Expand a fixed workload collection and bind runtime tools at each commit.

use serde::Serialize;

use crate::{Result, benchmark::Benchmark};

pub trait Suite: Serialize {
    type Tool;
    type Benchmark: Benchmark;

    fn benchmarks(&self) -> Result<Vec<Self::Benchmark>>;

    fn input(&self, tool: &Self::Tool) -> Result<<Self::Benchmark as Benchmark>::Input>;
}
