# Implementation guide

One library crate contains the phase contracts and concrete benchmarks. Two
binaries demonstrate the intended wiring with hard-coded parameters.
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
    sqlite.rs            SQLite backend placeholder
  view.rs                View contract and frontend series
  benchmarks/
    mcm/
      mod.rs             MCM suite, workload, options, runtime tools
      output.rs          Composite MCM results
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

The examples currently exit with explicit `NotImplemented` errors. They do not
build tools, launch benchmarks, open a database, or manufacture results.
`run_mcm` configures hard-coded coefficient sets; orchestration freezes them once
before resolving revisions.
Edit its repository, branch, commit anchor, library, and paper-tool paths when
the execution phases are implemented. `view_results` shows the typed query and
view pipeline; rendering is not implemented.

Working pieces are the types, serialization, suite expansion, tool binding,
reference-cache delegation, and sequential orchestration. The orchestration loop
registers frozen definitions and revision order, starts/finalizes attempts, records
benchmark failures, and completes only processed commits. Its tests use fake
dependencies; Git traversal, builds, timed execution, parsing,
reference storage, SQLite lifecycle operations, history projection, and graphical
rendering remain stubs. There is no SQLite dependency yet.

Each invocation currently creates a new experiment; automatic resume and input
digest verification remain unimplemented. Benchmark failures are terminal outcomes
and do not stop the sweep. Build, input setup, workspace, and storage failures
stop execution before completing the affected commit. Successful orchestration
means the comparisons were processed, not that every comparison succeeded.

With default retention, each attempt uses a temporary directory cleaned on return
or unwind. Requested artifacts keep a unique workspace under
`output/artifacts/<experiment>/attempt-*`, whose absolute path is recorded with
the attempt. The context/benchmark must select which files to retain; selective
file handling is still part of the unfinished context implementation.

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

1. Implement one coefficient-set MCM comparison: preparation, timed processes,
   parsing, and typed output. Use explicit coefficient sets before adding random
   generation; retain all repeated observations.
2. Add reference caching, including complete keys, immutable reference IDs,
   measurement environment, and refresh behavior.
3. Implement SQLite records and the typed MCM view for one commit, including the
   existing lifecycle contracts for registration, start/finish, build failures,
   and commit completion. Finalization must update the same started attempt.
4. Implement branch traversal and cached Yosys builds, then exercise the existing
   sequential history loop with those real dependencies.
5. Add verified resume, explicit reruns, input digest checks, and artifact
   inspection. These must not silently change workloads or replace measurements.
6. Add a graphical renderer, a synthesis view, and a design-manifest adapter.

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
  retain partial comparisons before implementing parsing/execution failures.
- Decide whether paper timing is reused across compatible sessions or refreshed
  each session while caching structural results separately. Choose the environment
  description and deterministic cache-key encoding before enabling reuse.
- Raw artifacts are off by default. Attempt directories are isolated and retained
  only on request; selective file retention and failure-only log policy remain
  context implementation decisions.
- Choose an explicit attempt-selection policy for views and resume. Preserve all
  attempts; retry observations must not be pooled as independent repetitions.
- Resolve the historical Verilog sizing questions documented with the reference
  before generating production workloads. Version changes affecting results.
- Typed history is the initial query boundary. Metric indexing, tables, coverage
  counts, and distributions can extend the minimal series projection as needed.
- Choose the initial design-manifest/preparation adapter and graphical frontend.
  Generated workloads must preserve actual values in addition to seeds.

These are documentation and implementation choices, not additional working
features. The examples continue to stop at explicit stub errors.
