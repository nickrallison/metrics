use std::time::Duration;

use super::{PaperStats, YosysStats};
use crate::{Error, Result, benchmarks::synth::SynthStats};

fn invalid(message: &str) -> Error {
    Error::Benchmark(format!("invalid tool output: {message}"))
}

/// Only the final top-module statistics block counts. Synthesis prints earlier
/// statistics too, and their cell rows must not inflate the final counts.
fn statistics(text: &str) -> Result<&str> {
    let header = "=== unoptimized ===";
    let (_, block) = text.rsplit_once(header).ok_or_else(|| invalid("missing top-module statistics"))?;
    Ok(block.split("\n=== ").next().unwrap_or(block))
}

fn cells(block: &str) -> Result<Vec<(u64, &str)>> {
    let mut result = Vec::new();
    let mut total = None;
    for line in block.lines() {
        if let Some(count) = line.trim().strip_prefix("Number of cells:") {
            let count = count.trim().parse::<u64>().map_err(|_| invalid("malformed cell total"))?;
            if total.replace(count).is_some() { return Err(invalid("duplicate cell total")); }
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        let Some(first) = fields.first() else { continue };
        let Ok(count) = first.parse::<u64>() else { continue };
        // Both older `N cells` and current `N area cells` layouts are supported.
        if (fields.len() == 2 || fields.len() == 3) && fields.last() == Some(&"cells") {
            if total.replace(count).is_some() { return Err(invalid("duplicate cell total")); }
            continue;
        }
        if total.is_some() && matches!(fields.len(), 2 | 3) {
            let cell = fields[fields.len() - 1];
            if cell == "wires" || cell == "bits" { continue; }
            // Area summary/footer lines begin with words rather than a count.
            result.push((count, cell));
        }
    }
    let expected = total.ok_or_else(|| invalid("missing cell total"))?;
    let sum = result.iter().try_fold(0u64, |sum, (count, _)| sum.checked_add(*count))
        .ok_or_else(|| invalid("cell count overflow"))?;
    if sum != expected { return Err(invalid("cell table does not match its total")); }
    Ok(result)
}

pub(super) fn yosys(text: &str, elapsed: Duration) -> Result<YosysStats> {
    let cells = cells(statistics(text)?)?;
    let count = |names: &[&str]| -> Result<u64> {
        cells.iter().filter(|(_, name)| names.contains(name))
            .try_fold(0u64, |sum, (value, _)| sum.checked_add(*value))
            .ok_or_else(|| invalid("operation count overflow"))
    };
    let mut depth = None;
    for line in text.lines().map(str::trim).filter(|line| line.starts_with("mcm:")) {
        if let Some((_, rest)) = line.split_once("adder(s), depth ") {
            let value = rest.split(',').next().unwrap_or(rest).trim().parse::<u32>()
                .map_err(|_| invalid("malformed MCM depth"))?;
            depth = Some(depth.map_or(value, |previous: u32| previous.max(value)));
        }
    }
    Ok(YosysStats { wall_clock_time: elapsed, adds_subs: count(&["$add", "$sub"])? ,
        negations: count(&["$neg"])? , multipliers: count(&["$mul"])? , depth })
}

pub(super) fn paper(text: &str, elapsed: Duration) -> Result<PaperStats> {
    let mut cost = None;
    let mut depth = None;
    let mut multipliers = 0;
    for line in text.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("// Cost: ") {
            let fields: Vec<_> = rest.split_whitespace().collect();
            if fields.len() != 6 || fields[1] != "adds/subtracts" || fields[3] != "shifts" || fields[5] != "negations" {
                return Err(invalid("malformed paper cost"));
            }
            let number = |index: usize| fields[index].parse::<u64>().map_err(|_| invalid("malformed paper count"));
            if cost.replace((number(0)?, number(2)?, number(4)?)).is_some() {
                return Err(invalid("duplicate paper cost"));
            }
        }
        if let Some(rest) = line.strip_prefix("// Depth: ") {
            let value = rest.parse::<u32>().map_err(|_| invalid("malformed paper depth"))?;
            if depth.replace(value).is_some() { return Err(invalid("duplicate paper depth")); }
        }
        if let Some((target, expression)) = line.split_once('=') {
            if target.trim().strip_prefix('t').is_some_and(|index| !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()))
                && expression.trim_start().starts_with("cmul(") {
                multipliers += 1;
            }
        }
    }
    let (adds_subs, shifts, negations) = cost.ok_or_else(|| invalid("missing paper cost"))?;
    Ok(PaperStats { wall_clock_time: elapsed, adds_subs, shifts, negations, multipliers,
        depth: depth.ok_or_else(|| invalid("missing paper depth"))? })
}

pub(super) fn synth(text: &str, elapsed: Duration) -> Result<SynthStats> {
    let block = statistics(text)?;
    let rows = cells(block)?;
    let cell_count = rows.iter().map(|(count, _)| count).sum();
    let mut area = None;
    for line in block.lines().map(str::trim).filter(|line| line.starts_with("Chip area for ")) {
        let (_, value) = line.rsplit_once(':').ok_or_else(|| invalid("malformed chip area"))?;
        let value = value.trim().parse::<f64>().map_err(|_| invalid("malformed chip area"))?;
        if !value.is_finite() || value < 0.0 { return Err(invalid("nonfinite or negative chip area")); }
        if area.replace(value).is_some() { return Err(invalid("duplicate chip area")); }
    }
    Ok(SynthStats { wall_clock_time: elapsed, cell_count, area })
}
