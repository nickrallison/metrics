//! Compare the MCM pass, a fixed paper tool, and synthesis with/without MCM.

mod output;
mod view;

pub use output::{McmOutput, PaperStats, YosysStats};
pub use view::{McmMetric, McmSelection, McmView};

use std::{num::NonZeroU32, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    Error, Result,
    benchmark::{Benchmark, RunContext},
    build::Yosys,
    design::CellLibrary,
    store::ReferenceCache,
    suite::Suite,
};

pub const KIND: &str = "mcm";
pub const VERSION: u32 = 1;

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
        _input: &McmInput,
        _context: &mut RunContext<'_, C>,
    ) -> Result<McmOutput> {
        // TODO: Generate Verilog, profile MCM, obtain the paper reference,
        // then synthesize with/without MCM and collect repeated samples.
        Err(Error::NotImplemented("MCM comparison and parsing"))
    }
}
