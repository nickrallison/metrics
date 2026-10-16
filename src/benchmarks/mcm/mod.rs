//! Compare the MCM pass, a fixed paper tool, and synthesis with/without MCM.
//!
//! One instance is one ordered signed coefficient set and its widths/options.
//! Generate its Verilog, profile the current Yosys MCM pass, obtain the paper
//! reference from the same coefficients, then synthesize with and without MCM.
//! The paper tool consumes coefficients and returns operation/DAG statistics;
//! it need not consume or emit Verilog. Keep all comparisons in this benchmark.
//!
//! `examples/reference/mcm.py` preserves the original commands, parsers, generator,
//! and formulas. Its companion README records historical quirks and changes
//! required by the scaffold's timing, repetition, caching, and retention contracts.
//! Preserve widths, signedness, order, and options across each comparison.
//!
//! Preparation is outside subprocess timing. Repetition strategy belongs here;
//! cache the independent paper observations, not the composite Yosys output.
//! Current output models successful comparisons. A phase error fails the whole
//! comparison unless partial-result fields are explicitly introduced later.

mod output;
mod parse;
mod run;
mod view;

pub use output::{McmOutput, PaperStats, YosysStats};
pub use view::{McmMetric, McmSelection, McmView};

use std::{num::NonZeroU32, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    Result,
    benchmark::{Benchmark, RunContext},
    build::Yosys,
    design::CellLibrary,
    store::ReferenceCache,
    suite::Suite,
};

pub const KIND: &str = "mcm";
// Version 2 uses full signed coefficient literals and full-width products.
pub const VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperTool {
    pub executable: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McmOptions {
    pub max_depth: u32,
    pub search_budget: u64,
    pub paper_seed: u64,
    pub paper_bit_width: u32,
}

/// Explicit workloads first; seeded generation can be added here later.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McmSuite {
    pub coefficient_sets: Vec<Vec<i64>>,
    pub input_width: u32,
    pub library: CellLibrary,
    pub paper: PaperTool,
    pub options: McmOptions,
    pub repetitions: NonZeroU32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McmBenchmark {
    pub coefficients: Vec<i64>,
    pub input_width: u32,
    pub library: CellLibrary,
    pub options: McmOptions,
    pub repetitions: NonZeroU32,
}

#[derive(Debug, Clone)]
pub struct McmInput {
    pub yosys: Yosys,
    pub paper: PaperTool,
}

impl Suite for McmSuite {
    type Tool = Yosys;
    type Benchmark = McmBenchmark;

    fn benchmarks(&self) -> Result<Vec<McmBenchmark>> {
        Ok(self
            .coefficient_sets
            .iter()
            .map(|coefficients| McmBenchmark {
                coefficients: coefficients.clone(),
                input_width: self.input_width,
                library: self.library.clone(),
                options: self.options.clone(),
                repetitions: self.repetitions,
            })
            .collect())
    }

    fn input(&self, tool: &Yosys) -> Result<McmInput> {
        Ok(McmInput {
            yosys: tool.clone(),
            paper: self.paper.clone(),
        })
    }
}

impl Benchmark for McmBenchmark {
    type Input = McmInput;
    type Output = McmOutput;

    fn kind(&self) -> &'static str {
        KIND
    }

    fn version(&self) -> u32 {
        VERSION
    }

    fn run<C: ReferenceCache>(
        &self,
        input: &McmInput,
        context: &mut RunContext<'_, C>,
    ) -> Result<McmOutput> {
        run::run(self, input, context)
    }
}
