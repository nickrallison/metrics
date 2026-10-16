use std::{fs, path::{Path, PathBuf}, process::Command};

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{McmBenchmark, McmInput, McmOutput, PaperStats, VERSION, parse};
use crate::{Error, Result, benchmark::{RunContext, Samples}, store::{Reference, ReferenceCache}};

pub(super) fn run<C: ReferenceCache>(benchmark: &McmBenchmark, input: &McmInput, context: &mut RunContext<'_, C>) -> Result<McmOutput> {
    validate(benchmark)?;
    let yosys = executable(&input.yosys.executable)?;
    let paper = executable(&input.paper.executable)?;
    let liberty = benchmark.library.liberty.canonicalize().map_err(|error| Error::Benchmark(format!(
        "cannot read Liberty file {}: {error}", benchmark.library.liberty.display()
    )))?;
    let scratch = tempfile::tempdir_in(&context.workspace)?;
    let workspace = scratch.path();
    let source = verilog(benchmark)?;
    fs::write(workspace.join("design.v"), &source)?;
    if context.artifacts.scripts { fs::write(context.workspace.join("design.v"), source)?; }
    let mcm = format!("mcm -depth {} -search_budget {}\n", benchmark.options.max_depth, benchmark.options.search_budget);
    let profile = format!("read_verilog design.v\n{mcm}stat\n");
    let synthesis = |with_mcm: bool| -> Result<String> { Ok(format!(
        "read_verilog design.v\nhierarchy -top unoptimized\n{}synth -top unoptimized -noabc\nabc -liberty {}\nstat -liberty {}\n",
        if with_mcm { &mcm } else { "" }, quote(&liberty)?, quote(&liberty)?
    )) };
    let baseline = synthesis(false)?;
    let optimized = synthesis(true)?;
    let mut yosys_mcm = Samples { values: Vec::new() };
    for index in 0..benchmark.repetitions.get() {
        let result = run_yosys(context, &yosys, workspace, &format!("mcm-{index}.ys"), &profile)?;
        yosys_mcm.values.push(parse::yosys(stdout(&result.output.stdout)?, result.elapsed)?);
    }
    let paper = reference(benchmark, &paper, workspace, context)?;
    let mut synth_without_mcm = Samples { values: Vec::new() };
    let mut synth_with_mcm = Samples { values: Vec::new() };
    for (name, script, samples) in [
        ("baseline", &baseline, &mut synth_without_mcm),
        ("with-mcm", &optimized, &mut synth_with_mcm),
    ] {
        for index in 0..benchmark.repetitions.get() {
            let result = run_yosys(context, &yosys, workspace, &format!("{name}-{index}.ys"), script)?;
            samples.values.push(parse::synth(stdout(&result.output.stdout)?, result.elapsed)?);
        }
    }
    if context.artifacts.netlists {
        // Separate diagnostic invocations keep write_verilog outside every
        // stored timing observation. They are not extra independent samples.
        for (name, script) in [("mcm", &profile), ("baseline", &baseline), ("with-mcm", &optimized)] {
            let path = context.workspace.join(format!("{name}.v"));
            let script = format!("{script}write_verilog {}\n", quote(&path)?);
            run_yosys(context, &yosys, workspace, &format!("inspect-{name}.ys"), &script)?;
        }
    }
    Ok(McmOutput { yosys_mcm, paper, synth_without_mcm, synth_with_mcm })
}

fn validate(benchmark: &McmBenchmark) -> Result<()> {
    let invalid = |message: &str| Error::Benchmark(message.into());
    if benchmark.coefficients.is_empty() { return Err(invalid("coefficient set is empty")); }
    if benchmark.input_width == 0 || benchmark.input_width > i32::MAX as u32 - 33 {
        return Err(invalid("input width is outside the supported signed Verilog range"));
    }
    if !(1..=30).contains(&benchmark.options.paper_bit_width) {
        return Err(invalid("paper bit width must be between 1 and 30"));
    }
    if benchmark.options.paper_seed > i32::MAX as u64 {
        return Err(invalid("paper seed exceeds its signed 32-bit command-line range"));
    }
    if benchmark.options.max_depth == 0 || benchmark.options.max_depth > i32::MAX as u32
        || benchmark.options.search_budget == 0 || benchmark.options.search_budget > i64::MAX as u64 {
        return Err(invalid("MCM depth and search budget must be positive signed integers"));
    }
    for &coefficient in &benchmark.coefficients {
        if coefficient == 0 {
            return Err(Error::Unsupported("the paper tool does not support zero coefficients".into()));
        }
        if i32::try_from(coefficient).is_err()
            || coefficient.unsigned_abs() >= (1u64 << (benchmark.options.paper_bit_width + 1)) {
            return Err(invalid("coefficient exceeds the paper tool's integer/search range"));
        }
    }
    Ok(())
}

fn verilog(benchmark: &McmBenchmark) -> Result<String> {
    // A positive signed magnitude needs its own sign bit, including when unary
    // minus is applied. Product width is the sum of the signed operand widths.
    let widths: Vec<_> = benchmark.coefficients.iter().map(|value| {
        (64 - value.unsigned_abs().leading_zeros()).max(1) + 1
    }).collect();
    let mut ports = vec![format!("    input wire signed [{}:0] x", benchmark.input_width - 1)];
    for (index, width) in widths.iter().enumerate() {
        let product = benchmark.input_width.checked_add(*width)
            .ok_or_else(|| Error::Benchmark("product width overflow".into()))?;
        ports.push(format!("    output wire signed [{}:0] y{index}", product - 1));
    }
    let mut source = format!("module unoptimized(\n{}\n);\n", ports.join(",\n"));
    for (index, (&coefficient, width)) in benchmark.coefficients.iter().zip(widths).enumerate() {
        let sign = if coefficient < 0 { "-" } else { "" };
        source.push_str(&format!("    assign y{index} = x * ({sign}{width}'sd{});\n", coefficient.unsigned_abs()));
    }
    source.push_str("endmodule\n");
    Ok(source)
}

fn run_yosys<C: ReferenceCache>(context: &mut RunContext<'_, C>, executable: &Path, workspace: &Path, name: &str, script: &str) -> Result<crate::benchmark::ProcessOutput> {
    fs::write(workspace.join(name), script)?;
    if context.artifacts.scripts { fs::write(context.workspace.join(name), script)?; }
    let result = context.run_process(Command::new(executable).current_dir(workspace).args(["-Q", "-T", "-s", name]));
    match result {
        Err(Error::Process { detail, .. }) if detail.contains("No such command: mcm") => {
            Err(Error::Unsupported("this Yosys revision does not provide the mcm pass".into()))
        }
        other => other,
    }
}

#[derive(Serialize)]
struct PaperKey<'a> {
    procedure: u32,
    executable_sha256: String,
    coefficients: &'a [i64],
    input_width: u32,
    bit_width: u32,
    seed: u64,
    repetitions: u32,
    scope: &'a Option<String>,
    os: &'static str,
    arch: &'static str,
}

fn reference<C: ReferenceCache>(benchmark: &McmBenchmark, executable: &Path, workspace: &Path, context: &mut RunContext<'_, C>) -> Result<Reference<Samples<PaperStats>>> {
    let key = serde_json::to_string(&PaperKey {
        procedure: VERSION, executable_sha256: format!("{:x}", Sha256::digest(fs::read(executable)?)),
        coefficients: &benchmark.coefficients, input_width: benchmark.input_width,
        bit_width: benchmark.options.paper_bit_width, seed: benchmark.options.paper_seed,
        repetitions: benchmark.repetitions.get(), scope: &context.reference_scope,
        os: std::env::consts::OS, arch: std::env::consts::ARCH,
    })?;
    if context.reference_scope.is_some() {
        if let Some(reference) = context.reference::<Samples<PaperStats>>(&key)? {
            if reference.value.values.len() != benchmark.repetitions.get() as usize {
                return Err(Error::Benchmark("cached paper sample count differs from its key".into()));
            }
            return Ok(reference);
        }
    }
    let mut samples = Samples { values: Vec::new() };
    for _ in 0..benchmark.repetitions.get() {
        let mut command = Command::new(executable);
        command.current_dir(workspace).args(["-b", &benchmark.options.paper_bit_width.to_string()]);
        command.args(benchmark.coefficients.iter().map(ToString::to_string));
        command.args(["-gc", "-seed", &benchmark.options.paper_seed.to_string()]);
        let result = context.run_process(&mut command)?;
        samples.values.push(parse::paper(stdout(&result.output.stdout)?, result.elapsed)?);
    }
    let id = context.cache_reference(&key, &samples)?;
    let reference = context.reference(&key)?.ok_or_else(|| Error::Storage("inserted reference is missing".into()))?;
    if reference.id != id { return Err(Error::Storage("reference changed during insertion".into())); }
    Ok(reference)
}

fn executable(path: &Path) -> Result<PathBuf> {
    // A bare command name can be resolved through PATH; explicit relative paths
    // are resolved before changing the subprocess working directory.
    if path.components().count() == 1 && !path.is_absolute() {
        if let Some(paths) = std::env::var_os("PATH") {
            for directory in std::env::split_paths(&paths) {
                let candidate = directory.join(path);
                if candidate.is_file() { return Ok(candidate.canonicalize()?); }
            }
        }
    }
    path.canonicalize().map_err(|error| Error::Benchmark(format!("cannot find tool {}: {error}", path.display())))
}

fn quote(path: &Path) -> Result<String> {
    let path = path.to_str().ok_or_else(|| Error::Benchmark("Yosys paths must be UTF-8".into()))?;
    if path.chars().any(char::is_control) { return Err(Error::Benchmark("control character in Yosys path".into())); }
    Ok(format!("\"{}\"", path.replace('\\', "\\\\").replace('"', "\\\"")))
}

fn stdout(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes).map_err(|error| Error::Benchmark(format!("tool output is not UTF-8: {error}")))
}

#[cfg(test)]
mod tests;
