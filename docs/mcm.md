# MCM execution and results

```sh
cargo run --bin run_mcm
cargo run --bin run_mcm -- --jobs 8
cargo run --bin view_results
cargo run --bin view_results -- --experiment 7 --metric area --json
cargo run --bin run_mcm -- --output-dir output --name manual-mcm --keep-artifacts
cargo run --bin view_results -- --output-dir output --name manual-mcm --details
```

The runner uses hard-coded repository, branch, revision selection, coefficients,
Liberty file, and paper executable paths in `src/bin/run_mcm.rs`. Its default paper
executable and library come from `../yosys/tests/techmap/mcm`. Both must exist;
the runner does not download them. A Yosys revision without `mcm` produces an
unsupported outcome. Each invocation creates a new experiment and preserves old
attempts, including failed scaffold runs.

The current selection is two adjacent mainline revisions, `8802950c22d4` then
`a31d95ea005b`, to check compiler cache reuse. To compile both instead of reusing
existing installations, choose a fresh output root:

```sh
cargo run --bin run_mcm -- --output-dir output-ccache-check
```

Both builds share `output-ccache-check/builds/ccache`. Each build log under
`output-ccache-check/builds/yosys/logs` ends compilation with cumulative ccache
statistics. The second build should add cache hits; dependencies that changed
still compile. Repeating the command reuses the completed installations.

Yosys compilation uses the available CPU count by default. `--jobs N` sets a
positive compiler job limit, capped at the available CPU count; see
[Yosys builds](builds.md).

For each workload and commit, the benchmark generates signed Verilog, profiles
MCM, obtains the paper comparison, then synthesizes without and with MCM. Each
phase runs the configured repetition count and retains every parsed observation
and wall-clock duration. MCM timings include Yosys startup, Verilog loading, the
MCM pass, statistics, and output collection. Synthesis timings include the whole
synthesis process. Preparation, parsing, cache access, and log writing are outside
the measured interval. No benchmark subprocesses run concurrently.

Synthesis follows the preserved MCM reference flow: hierarchy, optional MCM,
`synth -noabc`, `abc -liberty`, and `stat -liberty`. Only the final top-module
statistics table contributes counts/area. Missing area remains absent; malformed
statistics or unsuccessful processes fail the comparison. A later phase failure
currently makes the whole comparison fail. Failures are printed during execution
and retained in SQLite.

Procedure version 2 fixes the prototype's signed literal/product sizing. Each
literal includes its sign bit and full magnitude; each output can hold the full
signed product. Both tools receive the same ordered signed coefficient values.
The current paper adapter accepts nonzero signed 32-bit coefficients within its
configured search range, bit widths 1 through 30, and signed 32-bit seeds. Zero
coefficients are explicitly unsupported by the paper tool. Version 1 and version
2 use the same output shape, but their procedure histories are kept separate.

Paper references are reused only within one experiment, across Yosys commits.
Their key includes procedure version, executable content SHA-256, ordered values,
widths, seed, repetition count, OS/architecture, and experiment scope. New
experiments refresh timings. Reuse preserves the reference ID, timestamp, and
sample count. Unscoped contexts always measure fresh samples. Cross-experiment
timing reuse, resume, general input digests, and process timeouts are deferred.

Raw artifacts default off. Temporary source/scripts are removed on return,
including failure. Requested logs retain each process's stdout/stderr; requested
scripts retain the generated input and phase scripts. Requested netlists come
from separate diagnostic invocations after measured runs, so output generation
never changes stored timing observations. Diagnostic invocations add no samples.
`--keep-artifacts` enables logs, scripts, and netlists for the example. Retained
files follow the [output layout](outputs.md) selected by `--output-dir` and `--name`.

The viewer defaults to `output/mcm/database.sqlite`, the latest registered
experiment, and both metrics. Use `--output-dir PATH`, `--name NAME`,
`--database PATH`, `--experiment ID`, `--metric runtime|area|all`, and `--json` as
needed. Text shows values and included/selected workload counts; JSON exports the
same series with full commit IDs, nullable values, and exclusion reasons. Missing,
failed, unsupported, empty, or invalid measurements are explicit exclusions.
Commits without finished attempts still get missing points, in frozen order.
Older data at `output/mcm.sqlite` remains accessible with
`cargo run --bin view_results -- --database output/mcm.sqlite`.

Runtime ratio averages repetitions per workload, then divides mean Yosys runtime
by mean paper runtime on the same matched workload set. It does not average
per-workload ratios. Values below 1 favor Yosys. Area savings averages
`100 * (1 - mean(with-MCM area) / mean(baseline area))` over valid workloads.
Positive percentages favor MCM. Workloads have equal weight regardless of sample
count. Zero denominators are excluded; no included workloads yields `n/a`/`null`.
Coverage may differ between metrics and commits. Standard deviations and quality
series are not implemented yet.

Normal tests use small subprocess fixtures and SQLite. With the configured native
tools available, this optional check runs the real comparison and verifies signed
arithmetic at boundary input values:

```sh
cargo test --lib benchmarks::mcm::run::tests::real_comparison_and_signed_arithmetic -- --ignored --nocapture
```
