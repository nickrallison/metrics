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

# Further Brain Thoughts

- Collapse Criteria
  - when to decide two commits have effectively the same outcomes
- Verilog Files
  - How to store / get test suite
