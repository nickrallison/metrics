use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{benchmark::Samples, benchmarks::synth::SynthStats, store::Reference};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McmOutput {
    pub yosys_mcm: Samples<YosysStats>,
    pub paper: Reference<Samples<PaperStats>>,
    pub synth_without_mcm: Samples<SynthStats>,
    pub synth_with_mcm: Samples<SynthStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YosysStats {
    pub wall_clock_time: Duration,
    pub adds_subs: u64,
    pub negations: u64,
    pub multipliers: u64,
    pub depth: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperStats {
    pub wall_clock_time: Duration,
    pub adds_subs: u64,
    pub shifts: u64,
    pub negations: u64,
    pub multipliers: u64,
    pub depth: u32,
}
