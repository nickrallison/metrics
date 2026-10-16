# Yosys perf recordings

```sh
cargo run --bin run_mcm -- --workload 0 --commit f47cc3bfa0f4
```

Profiling is enabled by default in `run_mcm`; use `--perf=false` to disable it.
Bare `--perf` and `--perf=true` also enable it. Profiling runs the normal
measurements, then extra diagnostic Yosys invocations
for MCM, baseline synthesis, and synthesis with MCM. Each phase and repetition
gets its own recording and flamegraph. The paper reference is not profiled.
Recording and rendering do not contribute to stored timing samples or add
samples. Profiling does not change experiment identity; attempts still append.

The runner retains these files even with `--artifacts none`:

```text
<benchmark>/build_artifacts/perf/
  mcm-0/
    design.v
    run.ys
    perf.data
    flamegraph.svg
    commands.log
    yosys.stdout.log
    record.stderr.log
    script.stderr.log
    collapse.stderr.log
    flamegraph.stderr.log
  baseline-0/...
  with-mcm-0/...
```

Invocation `run.json` records the resolved profiling tools. The attempt's stored
artifact path points to `build_artifacts`. Intermediate decoded stacks are
temporary. Raw recordings, scripts, and diagnostic logs survive failures.
Any failing profiling stage fails that benchmark attempt. Very short processes
can yield no stacks; that is reported as a profiling error instead of generating
an empty flamegraph.

The recording command uses user-space CPU-clock samples at 999 Hz with DWARF
call graphs, so optimized code does not require frame pointers:

```sh
perf record -q -e cpu-clock:u -F 999 --call-graph dwarf,16384 \
  -o perf.data -- /path/to/yosys -Q -T -s run.ys
```

Rendering follows the requested command. Rust executes each stage with temporary
files and checks every exit status, so a failed upstream stage cannot silently
produce a successful SVG:

```sh
DEBUGINFOD_URLS= perf script -i perf.data \
  --addr2line=/usr/bin/llvm-addr2line-16 --no-inline -F +srcline \
  | stackcollapse-perf.pl --srcline | flamegraph.pl > flamegraph.svg
```

## Prerequisites

- Linux with access to perf events for the current user's processes. Containers
  may additionally restrict `perf_event_open`; host permissions still apply.
- A kernel-compatible `perf` executable with DWARF call-chain unwinding support.
  Check `perf version --build-options` for `dwarf-unwind` support via libdw or
  libunwind. Distribution builds normally supply the runtime libraries.
- LLVM's addr2line, defaulting to `/usr/bin/llvm-addr2line-16`. Select another
  installation with `--perf-addr2line /path/to/llvm-addr2line`.
- Perl and both scripts from [FlameGraph](https://github.com/brendangregg/FlameGraph)
  on PATH. The scripts are run using Perl; no additional CPAN modules are needed.
- Optimized Yosys binaries with debug information. The builder now uses CMake
  `RelWithDebInfo`; historical Make installs retain their debug symbols.

On Ubuntu 24.04, LLVM 16 is provided by `llvm-16`. Install Perl and the Linux tools
package matching your running kernel:

```sh
sudo apt install llvm-16 perl linux-tools-common "linux-tools-$(uname -r)"
```

Add a FlameGraph checkout to PATH, for example:

```sh
export PATH="$HOME/Applications/FlameGraph:$PATH"
```

Tools and script syntax are checked before opening the results database. Kernel
permissions are checked when recording starts; failures include the saved logs.
No automatic privilege escalation or system-setting changes are performed.
`DEBUGINFOD_URLS` is cleared for symbolization, so no debuginfod server is needed.

Check the real recording/rendering pipeline without building Yosys:

```sh
cargo test --offline --lib benchmark::perf::tests::native_flamegraph_contains_source_lines -- --ignored
```

This compiles a small C program with debug information and checks that the SVG
contains its source line. It was verified with the installed perf, LLVM 16, and
FlameGraph scripts.

Builds use `builds/yosys-relwithdebinfo` with recipe version 2. Existing Release
installations in `builds/yosys` remain available but are not reused by `run_mcm`.
The sibling ccache directory remains shared. See [build details](builds.md).

Sources: [perf record](https://man7.org/linux/man-pages/man1/perf-record.1.html),
[perf script](https://man7.org/linux/man-pages/man1/perf-script.1.html),
[perf permissions](https://docs.kernel.org/admin-guide/perf-security.html),
[LLVM addr2line](https://llvm.org/docs/CommandGuide/llvm-addr2line.html), and
[Ubuntu's LLVM 16 package](https://packages.ubuntu.com/noble/llvm-16).
