# Implementation guide

One library crate contains the phase contracts and concrete benchmarks. Two
binaries demonstrate MCM execution and result viewing.
This guide, module documentation, and the preserved reference experiment contain
the information needed to continue implementation independently of design drafts.

```text
src/
  lib.rs
  history.rs             Branch selection and commit metadata
  build/
    mod.rs               Builder contract
    yosys.rs             Yosys installation and builder
  benchmark/
    mod.rs               Benchmark contract and samples
    context.rs           Processes, workspaces, reference access, retention
  suite.rs               Suite contract
  experiment.rs          Sequential history orchestration
  design.rs              Verilog inputs and cell libraries
  store/
    mod.rs               Store contract, records, typed history
    reference.rs         Independent reference cache
    sqlite.rs            SQLite backend and typed history selection
    sqlite/schema.sql    Versioned storage schema
  view.rs                View contract and frontend series
  benchmarks/
    mcm/
      mod.rs             MCM suite, workload, options, runtime tools
      output.rs          Composite MCM results
      run.rs             Preparation, repetitions, and reference reuse
      parse.rs           Yosys, paper, and synthesis output parsing
      view.rs            MCM history projection
    synth.rs             Design/library sweep and synthesis results
  bin/
    run_mcm.rs           Configure and run an MCM experiment
    view_results.rs      Query MCM history and project results
```

```sh
cargo check --all-targets
cargo doc --no-deps
cargo run --bin run_mcm
cargo run --bin view_results
```

`run_mcm` opens/creates its database, builds each selected Yosys commit, and runs
the complete MCM comparison. `view_results` reads the latest registered
experiment by default and prints runtime ratios, area savings, coverage, and
exclusion reasons. Use `--experiment ID`, `--metric runtime|area|all`,
`--database PATH`, or `--json` to change its selection/output. Unknown experiments
and missing databases produce explicit errors.
`run_mcm` configures hard-coded coefficient sets; orchestration freezes them once
before resolving revisions.
Edit its repository, branch, commit anchor, library, and paper-tool paths for your
environment. Text rendering and JSON export are implemented; graphical rendering
is deferred.

Working pieces are the types, serialization, suite expansion, tool binding,
reference storage, typed SQLite history, Git revision resolution, cached Yosys
builds, timed MCM execution/parsing, MCM projections, and sequential orchestration.
The orchestration loop registers frozen
definitions and revision order, starts/finalizes attempts, records
benchmark failures, and completes only processed commits. Tests cover fake tool
dependencies with real SQLite, database reopen/interruption behavior, and Git
resolution against temporary repositories. The generic synthesis benchmark and
graphical rendering remain stubs.

See [SQLite storage](storage.md) for schema layout, lifecycle rules, reference
caching, history selection, and the fallible `SqliteStore::open` constructor.
See [Yosys builds](builds.md) for prerequisites, cache layout, logs, and the
optional real-build check.
See [MCM execution and results](mcm.md) for sampling, reference reuse, retention,
version boundaries, and viewer usage.

Revision resolution uses the installed Git CLI without fetching, checking out, or
modifying the worktree. It accepts local branch names and full `refs/heads/` or
`refs/remotes/` references. Selections are canonicalized to full commit IDs;
duplicate aliases and commits outside the captured first-parent history are
rejected. Metadata includes committer time, author name/email, and full messages.
Text is requested as UTF-8; undecodable output is an explicit error. The commands
use Git's documented [history formatting](https://git-scm.com/docs/git-log) and
[revision verification](https://git-scm.com/docs/git-rev-parse) interfaces.

Each invocation currently creates a new experiment; automatic resume and input
digest verification remain unimplemented. Benchmark failures are terminal outcomes
and do not stop the sweep. Build, input setup, workspace, and storage failures
stop execution before completing the affected commit. Successful orchestration
means the comparisons were processed, not that every comparison succeeded.

`Experiment::output` selects an `OutputLayout` with an output directory and a
human experiment name. Each invocation creates a UTC RFC 3339 timestamp directory.
With default retention, temporary attempt workspaces beneath it are cleaned on
return or unwind. Requested artifacts go directly into
`<output_dir>/<name>/<timestamp>/<index>-<kind>-<commit>/build_artifacts/`;
the absolute path is recorded with the attempt. MCM retains only requested logs,
scripts/input design, and diagnostic netlists. See [Output layout](outputs.md).

## Behavior contracts

Module documentation describes intended behavior, including behavior that is not
implemented yet. The main boundaries are:

| Location | Contract |
| --- | --- |
| [history](../src/history.rs) | Capture a branch tip, exclude the `Since` anchor, preserve order, follow the main line, validate explicit selections |
| [build](../src/build/mod.rs) | Repository-scoped commit cache, fixed build setup, validation, build failure handling |
| [suite](../src/suite.rs) | Freeze the sweep once; retain generated values and reuse the same workloads at each commit |
| [benchmark](../src/benchmark/mod.rs) | One complete comparison; benchmark-owned tools, parsing, repetitions, and procedure version |
| [context](../src/benchmark/context.rs) | Process timing scope, temporary workspaces, default-off raw artifacts |
| [experiment](../src/experiment.rs) | Sequential execution, attempt lifecycle, completion prefix, explicit reruns |
| [design](../src/design.rs) | Input content digests and reproducibility checks |
| [reference cache](../src/store/reference.rs) | Tool/workload/settings keys, environment-sensitive timing reuse, immutable reference IDs |
| [store](../src/store/mod.rs) | Typed payloads, schema validation, storage-owned IDs, inspection exclusion |
| [view](../src/view.rs) | Typed domain projection, matching workloads, units, missing coverage, revision order |
| [MCM](../src/benchmarks/mcm/mod.rs) and [MCM view](../src/benchmarks/mcm/view.rs) | Multi-tool comparison and exact summary formulas |

The framework walks one branch and invokes the same benchmark collection at each
selected commit. A benchmark can run several related procedures with different
input formats. There is no separate variant/job graph or universal Verilog input
requirement. Initial contracts use generic methods and static dispatch.

[The MCM reference](../examples/reference/README.md) preserves the original Python
script, commands, parsing, generator parameters, and statistical definitions. Its
porting notes distinguish historical behavior from the intended Rust contracts.

## Implementation order

1. Extend explicit coefficient sets with reproducible random generation or a
   manifest adapter; preserve generated values in the frozen collection.
2. Extend the scalar MCM view with quality, depth, and distributions.
3. Add verified resume, explicit reruns, input digest checks, and artifact
   inspection. These must not silently change workloads or replace measurements.
4. Implement the generic synthesis benchmark/view and a graphical renderer.

Existing suite expansion and tool binding are working scaffolding. The next
steps do not require a scheduler, plugin registry, generic stage graph,
configurable build-profile hierarchy, or universal metric database.

## Checks for implemented phases

- The same ordered coefficients, widths, and signedness reach both tools.
- A paper reference is reused across two Yosys commits, while Yosys gets fresh
  measurements. Reference reuse never increases the paper sample count.
- Building and other subprocesses do not overlap measured runs.
- A failed comparison is visible; an interrupted attempt or build failure does
  not advance completion. Resume uses the frozen collection and branch lineage.
- A changed input file is detected even if its path and label are unchanged.
- Multiple attempts do not inflate repetition counts; inspection attempts are
  linked to their source and excluded from timing summaries.
- Missing values, zero denominators, unmatched workloads, and cache environment
  mismatches are handled explicitly.
- Default retention leaves no permanent logs/scripts/netlists, while preserving
  parsed results and timing samples. Requested outputs survive cleanup.

## Remaining decisions

- Main-line traversal is the proposed branch rule. Supporting merged side
  histories or multiple branches in one experiment remains deferred.
- Current MCM output is all-or-failure. Decide whether per-phase outcomes should
  retain partial comparisons after a later phase fails.
- Paper timings are refreshed each experiment and reused across its commits.
  Cross-session timing reuse or separate structural caching needs a compatible
  environment policy before being enabled.
- Raw artifacts are off by default. Attempt directories are isolated and retained
  only on request. A failure-only retention policy remains a possible extension.
- History selects the latest finished measurement per workload/commit and excludes
  inspections and unfinished attempts. Automatic resume still needs a runner
  policy; retries must not be pooled as independent repetitions.
- MCM version 2 fixes signed coefficient and product sizing. Further procedure
  changes affecting results need another explicit version boundary.
- Typed history is the initial query boundary. Metric indexing, tables, coverage
  counts, and distributions can extend the minimal series projection as needed.
- Choose the initial design-manifest/preparation adapter and graphical frontend.
  Generated workloads must preserve actual values in addition to seeds.

These are documentation and implementation choices, not additional working
features. Unfinished operations continue to return explicit stub errors.
