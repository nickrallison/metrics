//! Expand a fixed workload collection and bind runtime tools at each commit.
//!
//! Call `benchmarks` once per experiment, before walking commits. Persist the suite
//! configuration and expanded benchmark values and reuse that collection at every
//! commit. Generated workloads retain both the seed and actual values. A changed
//! workload collection starts a new experiment.
//!
//! Storage assigns IDs to frozen definitions; authors do not invent design or
//! library IDs. `input` binds the current built tool to any fixed supporting tools
//! once per commit. The associated tool must match the builder's output.
//!
//! A synthesis suite expands allowed design/library pairs; an MCM suite expands
//! coefficient sets. Preparation can use an external script or ordinary helpers.
//! No universal design representation or Verilog conversion is required.

use serde::Serialize;

use crate::{Result, benchmark::Benchmark};

pub trait Suite: Serialize {
    type Tool;
    type Benchmark: Benchmark;

    fn benchmarks(&self) -> Result<Vec<Self::Benchmark>>;

    fn input(&self, tool: &Self::Tool) -> Result<<Self::Benchmark as Benchmark>::Input>;
}
