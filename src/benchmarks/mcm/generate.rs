//! Seeded workload generation matching the Python reference's random draw order.

use std::ops::RangeInclusive;

use rand_mt::Mt;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CoefficientSets {
    Explicit(Vec<Vec<i64>>),
    Generated(CoefficientGenerator),
}

impl CoefficientSets {
    pub fn len(&self) -> usize {
        match self {
            Self::Explicit(values) => values.len(),
            Self::Generated(generator) => generator.count,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn generate(&self) -> Result<Vec<Vec<i64>>> {
        match self {
            Self::Explicit(values) => Ok(values.clone()),
            Self::Generated(generator) => generator.generate(),
        }
    }
}

/// Python-compatible integer seeding, randint/choice rejection, and Pareto draws.
/// Defaults reproduce all 40 coefficient sets in the reference script.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoefficientGenerator {
    pub seed: u64,
    pub count: usize,
    pub length_range: RangeInclusive<u32>,
    pub value_range: RangeInclusive<i64>,
    pub a: f64,
    pub scale: f64,
}

impl Default for CoefficientGenerator {
    fn default() -> Self {
        Self {
            seed: 42,
            count: 40,
            length_range: 1..=6,
            value_range: -2_147_483_648..=2_147_483_647,
            a: 1.3,
            scale: 1024.0,
        }
    }
}

impl CoefficientGenerator {
    pub fn generate(&self) -> Result<Vec<Vec<i64>>> {
        if self.length_range.is_empty()
            || *self.length_range.start() == 0
            || self.value_range.is_empty()
            || !self.a.is_finite()
            || self.a <= 1.0
            || !self.scale.is_finite()
            || self.scale <= 0.0
        {
            return Err(Error::Benchmark(
                "invalid MCM coefficient generator settings".into(),
            ));
        }
        let span = self.length_range.end() - self.length_range.start();
        let span = span
            .checked_add(1)
            .ok_or_else(|| Error::Benchmark("coefficient length range is too large".into()))?;
        let mut random = PythonRandom::new(self.seed);
        let mut workloads = Vec::with_capacity(self.count);
        for _ in 0..self.count {
            let length = self.length_range.start() + random.below(span);
            let mut coefficients = Vec::with_capacity(length as usize);
            for _ in 0..length {
                let mut accepted = None;
                // Bound rejection for impossible or impractically narrow ranges.
                for _ in 0..1_000_000 {
                    let pareto = (1.0 - random.random()).powf(-1.0 / (self.a - 1.0));
                    let magnitude = (self.scale * (pareto - 1.0)).trunc();
                    let sign = if random.below(2) == 0 { -1 } else { 1 };
                    if magnitude.is_finite() && magnitude < 9_223_372_036_854_775_808.0 {
                        let value = (magnitude as i64) * sign;
                        if self.value_range.contains(&value) {
                            accepted = Some(value);
                            break;
                        }
                    }
                }
                coefficients.push(accepted.ok_or_else(|| {
                    Error::Benchmark(
                        "coefficient generator exhausted rejection limit; widen the value range"
                            .into(),
                    )
                })?);
            }
            workloads.push(coefficients);
        }
        Ok(workloads)
    }
}

// rand_mt owns MT19937; this adapter preserves CPython's integer seed encoding,
// 53-bit random() construction, and getrandbits-based _randbelow consumption.
struct PythonRandom(Mt);

impl PythonRandom {
    fn new(seed: u64) -> Self {
        let mut key = vec![seed as u32];
        if seed >> 32 != 0 {
            key.push((seed >> 32) as u32);
        }
        Self(Mt::new_with_key(key))
    }

    fn random(&mut self) -> f64 {
        let high = self.0.next_u32() >> 5;
        let low = self.0.next_u32() >> 6;
        (f64::from(high) * 67_108_864.0 + f64::from(low)) / 9_007_199_254_740_992.0
    }

    fn below(&mut self, bound: u32) -> u32 {
        let bits = 32 - bound.leading_zeros();
        loop {
            let value = self.0.next_u32() >> (32 - bits);
            if value < bound {
                return value;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_42_matches_every_python_reference_coefficient() {
        let expected: Vec<Vec<i64>> =
            serde_json::from_str(include_str!("../../../tests/fixtures/mcm_seed42.json")).unwrap();
        assert_eq!(
            CoefficientGenerator::default().generate().unwrap(),
            expected
        );
    }

    #[test]
    fn settings_are_repeatable_serializable_and_validated() {
        let generator = CoefficientGenerator {
            count: 10,
            seed: u64::MAX,
            ..Default::default()
        };
        let decoded: CoefficientGenerator =
            serde_json::from_str(&serde_json::to_string(&generator).unwrap()).unwrap();
        assert_eq!(generator.generate().unwrap(), decoded.generate().unwrap());
        let mut invalid = generator;
        invalid.a = 1.0;
        assert!(invalid.generate().is_err());
        invalid.a = f64::NAN;
        assert!(invalid.generate().is_err());
        let explicit = CoefficientSets::Explicit(vec![vec![0, 3, 3, -1]]);
        assert_eq!(explicit.generate().unwrap(), vec![vec![0, 3, 3, -1]]);
    }
}
