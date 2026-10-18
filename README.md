# Prerequisites

- cargo, cache
- diesel_cli - for modifying the tables
  - `cargo install diesel_cli --version '~2.3' --locked   --no-default-features --features sqlite`
- Yosys build tools: Git, ccache, CMake, a C++ compiler, Make, Flex, Bison,
  and the selected revision's native dependencies. See [builds](docs/builds.md).
- Profiling (enabled by default in `run_mcm`; disable with `--perf=false`):
  Linux perf with DWARF unwinding, Perl, LLVM addr2line
  (defaults to `/usr/bin/llvm-addr2line-16`), and FlameGraph's
  `stackcollapse-perf.pl` and `flamegraph.pl` on PATH.
  See [profiling setup and usage](docs/profiling.md).

# Results graph

```sh
cd webapp
dx serve
```

The Dioxus app reads the shared `../output/catalog.sqlite` index, which points
to experiment-specific databases. Set `BENCHMARKS_CATALOG_DATABASE` to use
another index, or `BENCHMARKS_OUTPUT_DIR` to change the default results root.
Older experiment databases under that root are indexed on first load. The web
UI renders registered analyses from the store and uses Plotly only for the
interactive chart. See [MCM results](docs/mcm.md).

# Further Brain Thoughts

- Collapse Criteria
  - when to decide two commits have effectively the same outcomes
- Verilog Files
  - How to store / get test suite
