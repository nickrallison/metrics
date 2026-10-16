# Output layout

Configure one output root and a human experiment name:

```sh
cargo run --bin run_mcm -- --output-dir output --name mcm --keep-artifacts
cargo run --bin view_results -- --output-dir output --name mcm --details
```

The default root is `output` and default name is `mcm`. `--name` chooses a
filesystem/database namespace; the viewer's `--experiment ID` chooses an
registered experiment inside that database. Compatible invocations reuse its ID.

```text
output/
  builds/yosys/                       Shared Yosys installation cache and build logs
  builds/ccache/                      Shared compiler output across Yosys revisions
  mcm/
    database.sqlite                   SQLite results for this named experiment
    2026-10-16T18:42:07.123456789Z/     One invocation, in UTC
      run.json                       Invocation settings and experiment ID
      0000-mcm-<full-commit-id>/       Frozen workload index, kind, and revision
        build_artifacts/
          design.v
          mcm-0.ys
          baseline-0.ys
          with-mcm-0.ys
          process-000.stdout.log
          process-000.stderr.log
          mcm.v
          baseline.v
          with-mcm.v
          ...
      0001-mcm-<full-commit-id>/
        build_artifacts/
          ...
```

Timestamps use RFC 3339 with a fixed nine-digit fractional second and explicit
UTC `Z`: `YYYY-MM-DDTHH:MM:SS.nnnnnnnnnZ`. They are human-readable, sort
chronologically, and can be parsed using standard RFC 3339 parsers. Directory
creation is atomic. If a timestamp already exists, the allocator advances one
nanosecond until it creates a fresh directory, preserving existing files.

A timestamp covers the whole invocation. A benchmark directory includes its
original frozen position and full commit ID so duplicate workloads and runs over
multiple revisions have separate destinations. Paths are stored in SQLite;
the original `configuration.run_directory` identifies the first invocation,
`run.json` identifies each invocation and its settings, and each retained
attempt's `artifacts` field identifies its `build_artifacts` directory.

Logs and scripts are retained by default; netlists are opt-in.
`--artifacts none` disables retention, and `--keep-artifacts` enables all three flags;
library callers can select logs, scripts, and netlists independently through
`ArtifactPolicy`. With retention off, scratch workspaces inside the timestamp
directory are cleaned on return/unwind, and parsed observations remain in SQLite.
A timestamp directory still contains its small `run.json`. Retained files survive benchmark
failures, panics, and finalization failures. Netlists come from separate diagnostic
invocations and do not add timing observations.

`OutputLayout` validates the experiment name as one directory component.
`Experiment::output` selects the layout for execution. The supplied binaries also
use its database path; callers of the generic runner still provide their own
`Store`. The shared Yosys cache is configured separately by the builder and the
example puts it at `<output_dir>/builds/yosys` to reuse installations across runs.
Its sibling `<output_dir>/builds/ccache` reuses compiler output across commits;
see [Yosys builds](builds.md).

Earlier databases and artifact directories are preserved. Use
`view_results --database output/mcm.sqlite` to read the previous database layout.
`--database` overrides the viewer's output-root/name-derived database path.
