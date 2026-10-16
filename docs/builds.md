# Yosys builds

`YosysBuilder` accepts a resolved full commit ID and returns absolute paths to a
private Yosys executable and its data directory. It builds committed sources in a
temporary clone. It does not check out, stash, or copy edits from your worktree.
Local submodule object databases are reused when they contain the pinned commit;
missing dependencies are fetched using Git's normal submodule update behavior.
Only Yosys's top-level submodules are initialized, matching its build instructions.

The fixed procedure uses CMake Release, bundled dependency libraries, and no unit
tests. ABC and the default synthesis components are included. Revisions without
CMakeLists.txt use their historical Makefile defaults. Both procedures install
into the cache without sudo. Compilation defaults to the available CPU count;
set `YosysBuilder.jobs` to a nonzero count to lower the limit. The example runner
accepts `--jobs N`. Both cap the requested count at the available CPU count,
then pass it to CMake's `--parallel N` or Make's `-j N`:

```sh
cargo run --bin run_mcm -- --jobs 8
```

Changing the job count reuses existing cached installations. Commits still build
one at a time, and compilation finishes before any benchmark is timed.
Git, ccache, and the native build dependencies required by the selected Yosys
revision must be installed. A completed installation can still be reused without
ccache; it is checked only when compilation is needed.
Current CMake revisions require CMake 3.28+, a C++20 compiler, Make, Flex, Bison,
and the libraries described in the Yosys README. Missing commands, dependencies,
and compilation failures are reported with the stage and a persistent log path.

```text
output/builds/yosys/
  cache.json             Canonical repository path and build recipe version
  .lock                  Serializes builders sharing this cache
  <full-commit-id>/
    bin/yosys
    bin/yosys-abc
    share/yosys/         Installed technology libraries and supporting files
    .complete            Written only after installation passes validation
  logs/<commit>-*.log   Commands, stdout, and stderr for each builder invocation
```

Compiler output is shared between commits in `output/builds/ccache/`, beside the
installation cache. A custom `cache_dir` uses the same sibling `ccache` directory.
CMake uses its C/C++ compiler launchers (including Yosys's launcher option); the
historical Make build receives `ENABLE_CCACHE=1` for compilation and installation.
The builder sets `CCACHE_DIR` to that shared directory and `CCACHE_BASEDIR` to each
temporary workspace, normalizing compiler paths across fresh checkouts. Ccache
temporary files stay inside the workspace. These settings affect child processes
only; the user's global ccache configuration is not changed.

Unchanged compilations can reuse cached object files. Source/header, compiler,
and option changes are handled by ccache; configuration, code generation,
linking, and installation still run. Generated version information and embedded
installation paths can reduce cache hits. Cumulative ccache statistics are
appended to each successful compilation's build log. Inspect them directly with:

```sh
CCACHE_DIR="$PWD/output/builds/ccache" ccache --show-stats
```

Compiler caching and job limits do not invalidate existing installations. The
compiler cache starts warming when a previously unbuilt commit is compiled.

Source and compiler output directories are temporary and cleaned on return.
Build logs are retained independently of benchmark artifact settings. A failed or
interrupted installation has no completion marker and is rebuilt on the next
attempt. Before returning a cached installation, the builder checks required data
files, runs Yosys to read its installed simulation libraries, and starts ABC.
A failed validation triggers a fresh build.

A cache belongs to one canonical repository path and build recipe. Reusing it for
a different repository or recipe returns an error; select a different cache
directory. Changes to compilation options or installed contents must increment
`RECIPE_VERSION`. Compiler, operating-system, and external library changes are
not fingerprinted yet: use a new cache directory after changing the toolchain or
build environment. Builders sharing a cache wait for its file lock before
validating or modifying installations. The lock is released when the builder
returns or its process exits.

The normal test suite requires ccache and a C++ compiler in addition to CMake,
Make, and Git. Small committed fixtures check both build systems, reuse across
commits, header invalidation, revision isolation, local submodules, cache
validation, and failures. An optional
check compiles a real Yosys revision, verifies reuse, and synthesizes a small adder
using the installed data files and ABC:

```sh
cargo test --offline --lib build::yosys::tests::real_yosys_build -- --ignored --nocapture
```

It defaults to `../yosys`, `HEAD` on `main`, and `output/builds/yosys`. Override
`BENCHMARKS_YOSYS_REPOSITORY`, `BENCHMARKS_YOSYS_COMMIT`, or
`BENCHMARKS_YOSYS_CACHE` as needed. Cargo's `--offline` controls Cargo dependencies;
Git can still fetch missing Yosys submodules. The real check is ignored during
normal tests because it requires the full native build environment.
