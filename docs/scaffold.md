# Source layout

One library crate contains the phase contracts and concrete benchmarks. Two
binaries demonstrate the intended wiring with hard-coded parameters.

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
cargo run --bin run_mcm
cargo run --bin view_results
```

The examples currently exit with explicit `NotImplemented` errors. They do not
build tools, launch benchmarks, open a database, or manufacture results.
`run_mcm` expands its hard-coded coefficient sets before reaching orchestration.
Edit its repository, branch, commit anchor, library, and paper-tool paths when
the execution phases are implemented. `view_results` shows the typed query and
view pipeline; rendering is not implemented.

Working pieces are the types, serialization, suite expansion, tool binding, and
reference-cache delegation. Git traversal, builds, timed execution, parsing,
reference storage, SQLite lifecycle operations, history projection, and graphical
rendering remain stubs. There is no SQLite dependency yet.

See [planv2.md](../planv2.md) for the design and implementation order. Experiment
registration/completion, input digest checks, cache-key construction, and
inspection reruns belong to those unfinished operations.
