use std::{num::NonZeroU32, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{
    Error, Result,
    benchmark::{Benchmark, RunContext, Samples},
    build::Yosys,
    design::{CellLibrary, VerilogDesign},
    store::ReferenceCache,
    suite::Suite,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SynthOptions {
    pub additional_passes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthBenchmark {
    pub design: VerilogDesign,
    pub library: CellLibrary,
    pub options: SynthOptions,
    pub repetitions: NonZeroU32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthStats {
    pub wall_clock_time: Duration,
    pub cell_count: u64,
    pub area: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthSuite {
    pub designs: Vec<VerilogDesign>,
    pub libraries: Vec<CellLibrary>,
    pub options: SynthOptions,
    pub repetitions: NonZeroU32,
}

impl Suite for SynthSuite {
    type Tool = Yosys;
    type Benchmark = SynthBenchmark;

    fn benchmarks(&self) -> Result<Vec<SynthBenchmark>> {
        Ok(self
            .designs
            .iter()
            .flat_map(|design| {
                self.libraries.iter().map(|library| SynthBenchmark {
                    design: design.clone(),
                    library: library.clone(),
                    options: self.options.clone(),
                    repetitions: self.repetitions,
                })
            })
            .collect())
    }

    fn input(&self, tool: &Yosys) -> Result<Yosys> {
        Ok(tool.clone())
    }
}

impl Benchmark for SynthBenchmark {
    type Input = Yosys;
    type Output = Samples<SynthStats>;

    fn kind(&self) -> &'static str {
        "synth"
    }

    fn version(&self) -> u32 {
        1
    }

    fn run<C: ReferenceCache>(
        &self,
        _input: &Yosys,
        _context: &mut RunContext<'_, C>,
    ) -> Result<Self::Output> {
        Err(Error::NotImplemented("mapped synthesis and parsing"))
    }
}
