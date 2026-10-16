# MCM execution and results

```sh
cargo run --bin run_mcm
cargo run --bin run_mcm -- --jobs 8
cargo run --bin run_mcm -- --perf --workload 0
cargo run --bin view_results
cargo run --bin view_results -- --experiment 7 --metric area --json
cargo run --bin run_mcm -- --output-dir output --name manual-mcm --keep-artifacts
cargo run --bin view_results -- --output-dir output --name manual-mcm --details
```

The runner uses hard-coded repository, branch, revision selection, coefficients,
Liberty file, and paper executable paths in `src/bin/run_mcm.rs`. Its default paper
executable and library come from `../yosys/tests/techmap/mcm`. Both must exist;
the runner does not download them. A Yosys revision without `mcm` produces an
unsupported outcome. Compatible invocations reuse the same experiment ID and
append attempts. A changed frozen workload collection, procedure version, suite,
or branch/repository configuration starts a different experiment. Changing
artifact policy or selected commits does not.

Rerun one commit under an existing experiment, retaining all benchmark artifacts:

```sh
cargo run --bin run_mcm -- --experiment 5 --commit a31d95ea005b --keep-artifacts
```

Add `--workload 0` to rerun only that frozen workload. Both selectors are
repeatable. `--experiment ID` loads the saved suite and repository settings;
without it, the runner uses its configured suite and reuses the most recently
registered compatible experiment. Newly selected commits append to its revision
order. Existing experiments are not merged or renumbered.

`--artifacts all|none|logs,scripts,netlists` selects retention explicitly; a comma
list may contain any subset. `--keep-artifacts` enables all three kinds.
Artifact settings do not change benchmark parameters. Existing Yosys
installations and ccache remain reusable; rerunning measurements does not
necessarily repeat compilation.

Every attempt gets a record ID and preserves its commit, workload, samples,
outcome, and retained artifact path. New attempts never overwrite old results or
files. Normal views use the latest finished result per commit/workload, including
failures; an unfinished retry leaves the previous finished result visible.
List all finished attempts and unfinished work with:

```sh
cargo run --bin view_results -- --experiment 5 --attempts
cargo run --bin view_results -- --experiment 5 --attempts --json
```

The default selection follows `nick/mcm` and includes all 35 matching commits
on its first-parent history, oldest ancestor first. It includes all 32 `mcm:`
commits, the original MCM pass merge (`fb1a2fdae7a8`), and the release and
source-location refactor merges that changed MCM source (`70bf41c29bef` and
`ec3ddf1adf8d`). The final point is `f47cc3bfa0f4`.

Selection includes any commit whose message mentions MCM or whose changed paths
contain MCM, including files inside MCM directories. Test-only, profiling-only,
comment, and build-fix commits are all separate data points. The full immutable
IDs are pinned with descriptions in `run_mcm.rs`; the list does not automatically
expand when the branch advances. Ancestry determines execution order, since
rebased commits can have author dates older than their ancestors.
Changing the default branch from `main` to `nick/mcm` creates a different experiment
from the earlier mainline cache check. `--experiment ID` still loads that saved
experiment's branch and revisions.

To exercise compilation instead of reusing existing installations, choose a fresh
output root:

```sh
cargo run --bin run_mcm -- --output-dir output-ccache-check
```

All builds share `output-ccache-check/builds/ccache`. Each build log under
`output-ccache-check/builds/yosys-relwithdebinfo/logs` ends compilation with cumulative ccache
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

Paper references are reused only within one invocation, across Yosys commits.
Their key includes procedure version, executable content SHA-256, ordered values,
widths, seed, repetition count, OS/architecture, and invocation scope. Every
invocation refreshes timings, including reruns in the same experiment.
Reuse preserves the reference ID, timestamp, and
sample count. Unscoped contexts always measure fresh samples. Cross-experiment
timing reuse, automatic resume, general input digests, and process timeouts are deferred.

Logs and scripts are retained by default; netlists are opt-in. Unretained
temporary files are removed on return, including failure. Requested logs retain
each process's stdout/stderr; requested
scripts retain the generated input and phase scripts. Requested netlists come
from separate diagnostic invocations after measured runs, so output generation
never changes stored timing observations. Diagnostic invocations add no samples.
`--keep-artifacts` enables logs, scripts, and netlists for the example. Retained
files follow the [output layout](outputs.md) selected by `--output-dir` and `--name`.

Profiling is enabled by default (`--perf=false` disables it). It records
additional Yosys invocations after measured samples, generating
`perf.data` and `flamegraph.svg` for each phase/repetition. These artifacts are
retained independently of the normal retention policy. See [profiling](profiling.md)
for prerequisites, commands, and directory layout.

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
