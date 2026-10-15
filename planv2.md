# Benchmark history plan, version 2

Status: proposal for review, incorporating the notes between
`c2d6b309910f7fcc57a0b9d7afdb47af725fe6d0` and
`ab56896da386e9f70929739a28df69bcd4062c41`.

Rust sketches describe interfaces, not implementations. Imports, derives,
visibility, defaults, and supporting error/identifier types are omitted.

## Main change

A benchmark owns a complete comparison at one commit. It can invoke several
processes, use different input formats, choose repetition counts, parse their
outputs, and return one structured result.

For MCM, one benchmark instance represents a coefficient set and its settings.
At each Yosys commit it can compare the MCM pass, the paper tool, and synthesis
with and without MCM. These operations belong together inside the benchmark.

The surrounding framework walks one branch, obtains a tool for each commit,
executes the same benchmark collection sequentially, and stores typed results.

```text
Experiment
    BranchHistory + RevisionSelection
    Suite -> fixed collection of Benchmark instances

For each selected commit:
    Builder -> tool for this commit
    Suite -> runtime inputs containing that tool
    For each Benchmark:
        run complete comparison -> typed Output -> Store

Store -> History<Output> -> View -> graphical presentation
```

| Previous proposal                               | Revised proposal                                            |
| ----------------------------------------------- | ----------------------------------------------------------- |
| Variants and jobs express individual procedures | Benchmark implementation owns related procedures            |
| Yosys parameter is fixed in the benchmark trait | Associated `Input` supplies whatever tools are needed       |
| Concrete `Suite<I>` and `Case<I>` wrappers      | `Suite` produces user-defined benchmark instances           |
| Build specifications and provenance hierarchy   | One fixed build setup, cached by commit within a repository |
| Framework-wide repetition policy                | Benchmark owns repetition strategy                          |
| Mandatory artifact retention                    | Parsed results retained; raw files opt in                   |
| Concrete storage/query contracts                | Storage and view traits with typed results                  |

## 1. Branch history and revision selection

```rust
struct BranchHistory {
    repository: PathBuf,
    branch: String,
}

enum RevisionSelection {
    Explicit(Vec<CommitId>),
    Since(CommitId),
}

struct Commit {
    id: CommitId,
    committed_at: Timestamp,
    author: String,
    commit_message: String,
}
```

Assume one branch per experiment. The branch belongs to the parent structure;
revision selection only chooses states along its history.

Resolve the branch tip once at the start. Expand `Since(anchor)` to commits after
the anchor through that captured tip, oldest first. The anchor is excluded.
Reject an anchor outside the selected branch lineage rather than silently
selecting unrelated history.

Proposed merge rule: walk the branch's main line and do not separately benchmark
side-branch commits introduced by merges. This is an internal traversal rule,
not another range-selection type. Explicit lists identify commits on that same
lineage and preserve supplied order; reject duplicate entries.

The result of revision planning is simply `Vec<Commit>`. There is no generic job
graph or separately persisted plan type.

The store can find the prior completed commit for the same experiment. The caller
uses that ID to construct `Since(id)`. Completion means every benchmark in the
frozen collection has a recorded terminal outcome, including explicit failure
or unsupported outcomes. It does not imply all measurements succeeded.

Do not derive this position from the most recent individual result or wall-clock
timestamp. Do not advance a completion watermark over an unfinished commit.
Failed measurements remain visible and can be rerun through explicit selection.

## 2. Building a tool at a commit

```rust
trait Builder {
    type Tool;

    fn build(&mut self, commit: &Commit) -> Result<Self::Tool, BuildError>;
}

struct YosysBuilder {
    repository: PathBuf,
    cache_dir: PathBuf,
}

struct Yosys {
    commit: CommitId,
    executable: PathBuf,
    data_dir: PathBuf,
}
```

`YosysBuilder` implements `Builder<Tool = Yosys>`. Its build method may return an
existing cached installation. The repository and fixed build procedure belong
to the builder setup; the commit identifies an entry within that cache.

Start with one build configuration. Do not introduce `BuildId`, `BuildConfig`,
dependency policies, or compiler-specification types yet. Changing the build
procedure requires invalidating or separating its cache. Add configuration to
the key only when comparing multiple build setups becomes a requirement.

Keep installed files needed by Yosys together and validate a cached installation
before returning it. A failed build records the commit and error and does not
produce benchmark measurements.

Building happens outside benchmark timings and does not overlap measured runs.
Repository-specific caches prevent equal commit IDs in different repositories
from being treated as the same installation.

## 3. Suites produce benchmarks

```rust
trait Suite: Serialize {
    type Tool;
    type Benchmark: Benchmark;

    fn benchmarks(&self) -> Result<Vec<Self::Benchmark>, SuiteError>;

    fn input(
        &self,
        tool: &Self::Tool,
    ) -> Result<<Self::Benchmark as Benchmark>::Input, SuiteError>;
}
```

`Suite` owns parameter expansion and runtime-input construction. Its associated
benchmark type defines the workload and result shape; its associated tool type
must match the builder's output.

Generate the collection once, before walking commits. Record the suite settings
and expanded benchmark definitions, then reuse that collection at every commit.
Random workloads retain both their seed and generated values. A changed sweep
starts a new experiment rather than extending a history with different inputs.
Storage assigns each frozen benchmark definition a row ID to link its runs;
benchmark authors do not construct case, design, or library IDs.

There is no required `Case` type or universal design representation. A benchmark
instance already contains one workload and its procedure settings.

Examples:

- `SynthSuite` expands designs × cell libraries, subject to any compatibility
  filters, into `SynthBenchmark` instances.
- `McmSuite` expands generated coefficient sets into `McmBenchmark` instances.
  A library used by the synthesis comparison can be fixed or swept explicitly.

`Suite::input` runs once per commit. It binds that commit's built tool to fixed
supporting tools, such as the paper executable. It returns runtime handles rather
than choosing the workload.

## 4. Benchmark contract

```rust
trait Benchmark: Serialize {
    type Input;
    type Output: Serialize + DeserializeOwned;

    fn kind(&self) -> &'static str;
    fn version(&self) -> u32;

    fn run<C: ReferenceCache>(
        &self,
        input: &Self::Input,
        context: &mut RunContext<'_, C>,
    ) -> Result<Self::Output, BenchmarkError>;
}
```

The serialized benchmark value describes its workload and configuration.
`kind` and `version` distinguish benchmark definitions and output formats in the
database. The associated input is runtime-only and need not be serializable.

The benchmark decides:

- Which tools to invoke and in what order.
- How to translate its workload into each tool's input format.
- Which operations to repeat and how many times.
- How to parse results and form its composite output.
- Which independent reference computation can be cached.

The framework supplies a small `RunContext`: a temporary workspace, timed process
execution, optional artifact retention, and reference-cache access. Helpers can
repeat a closure and collect samples, but do not impose a repetition strategy.

Keep parsing in benchmark implementations or their ordinary helper functions.
No separate parser trait is required initially. Runtime belongs in typed output
when it is a benchmark metric; the process helper supplies the measured duration.

`ReferenceCache` is a generic parameter so context helpers can cache typed values
without giving the benchmark a database connection. These contracts use static
dispatch; their generic methods are not intended for trait objects.

## 5. MCM example

```rust
struct McmSuite {
    generator: CoefficientGenerator,
    library: CellLibrary,
    paper: PaperTool,
    repetitions: NonZeroU32,
}

struct McmBenchmark {
    coefficients: Vec<i64>,
    input_width: u32,
    library: CellLibrary,
    options: McmOptions,
    repetitions: NonZeroU32,
}

struct McmInput {
    yosys: Yosys,
    paper: PaperTool,
}

struct McmOutput {
    yosys_mcm: Samples<YosysStats>,
    paper: Reference<Samples<PaperStats>>,
    synth_without_mcm: Samples<SynthStats>,
    synth_with_mcm: Samples<SynthStats>,
}

struct Samples<T> {
    values: Vec<T>,
}

struct YosysStats {
    wall_clock_time: Duration,
    adds_subs: u64,
    negations: u64,
    multipliers: u64,
    depth: Option<u32>,
}

struct PaperStats {
    wall_clock_time: Duration,
    adds_subs: u64,
    shifts: u64,
    negations: u64,
    multipliers: u64,
    depth: u32,
}

struct SynthStats {
    wall_clock_time: Duration,
    cell_count: u64,
    area: Option<f64>,
}
```

`McmSuite` implements `Suite<Tool = Yosys, Benchmark = McmBenchmark>`.
`McmBenchmark` implements `Benchmark<Input = McmInput, Output = McmOutput>`.
Signed coefficients preserve the negative values present in the example script.

For one coefficient set at one commit, `run` performs:

1. Generate Verilog from the coefficients, widths, and signedness rules.
2. Profile the current Yosys MCM pass and parse operation counts and depth.
3. Obtain the paper tool's result for those same coefficients, using the reference
   cache when allowed. The paper tool does not need to consume or emit Verilog.
4. Synthesize the generated design without MCM and collect area and timing.
5. Synthesize the same design with MCM and collect area and timing.
6. Return the composite output, preserving individual repeated observations.

These are related measurements inside one benchmark, not separately selected
history variants. Preparation time is excluded from subprocess wall time.
An additional complete-comparison duration can be recorded if it is useful.

The sketch shows successful output. If retaining partial comparisons is needed,
individual fields can use `Result<_, PhaseFailure>` without changing the framework.
Otherwise a failed phase produces a failed benchmark record.

## 6. General synthesis and design preparation

```rust
struct VerilogDesign {
    top: String,
    sources: Vec<SourceFile>,
    include_dirs: Vec<PathBuf>,
    defines: BTreeMap<String, Option<String>>,
}

struct CellLibrary {
    name: String,
    liberty: PathBuf,
}

struct SynthBenchmark {
    design: VerilogDesign,
    library: CellLibrary,
    options: SynthOptions,
    repetitions: NonZeroU32,
}
```

For this benchmark, the associated input can simply be `Yosys`. The synthesis
suite constructs one benchmark per allowed design/library pair.

Remove separate `DesignId` and `LibraryId` fields. They do not need user-managed
construction. Readable names and serialized benchmark parameters identify inputs
for selection; database row IDs are assigned by storage.

File-backed inputs must remain fixed during an experiment. Record their content
digests alongside the serialized definitions and check them when resuming or
rerunning. A reused path does not prove that the workload is unchanged. This
does not require a public identity hierarchy for every domain type.

The existing manifest can be adapted into suite generation and preparation.
Preparation can initially be an external script or ordinary suite/benchmark
helper. Preserve generated values and source dependencies. There is no universal
`Design` trait or requirement that all workloads convert into Verilog.

## 7. Reference caching

```rust
trait ReferenceCache {
    fn get<T: DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Option<Reference<T>>, CacheError>;

    fn insert<T: Serialize>(
        &mut self,
        key: &str,
        value: &T,
    ) -> Result<ReferenceId, CacheError>;
}

struct Reference<T> {
    id: ReferenceId,
    value: T,
    measured_at: Timestamp,
}
```

The benchmark defines which computation is independent of the Yosys commit.
Cache the paper result separately and attach its reference ID to every comparison
that uses it. Small cached statistics may be embedded in the composite output;
the ID still makes reuse explicit.

The paper cache key includes tool identity, coefficients and their order, widths,
arguments, seed, parser version, and sampling policy. Verify the executable's
identity rather than keying only on its path. It excludes the Yosys commit and
settings irrelevant to the paper computation, such as the synthesis library.

Cached timing also depends on the measurement environment. Restrict its reuse to
an appropriate environment or refresh it explicitly. Record that environment in
the cache entry's metadata. A refresh creates a new reference ID; historical
comparisons keep their original reference.

Preserve all paper timing samples. Reusing their reference at another commit is
not another independent paper measurement. Yosys measurements are rerun at each
commit, so its performance relative to the fixed reference can still change.

## 8. Sequential execution

```rust
struct Experiment<S: Suite> {
    history: BranchHistory,
    revisions: RevisionSelection,
    suite: S,
    artifacts: ArtifactPolicy,
}
```

Use an ordinary generic orchestration function rather than a scheduler hierarchy.
Its bounds connect `S: Suite`, `B: Builder<Tool = S::Tool>`, and `D: Store`.

The execution order is:

1. Resolve commits and freeze the suite's benchmark collection.
2. For each commit, acquire its tool and construct the suite's runtime input.
3. Run each benchmark sequentially, including any repetitions it requests.
4. Save each comparison's outcome and mark commit completion after the collection
   is processed.

Run builds, reference computations, and other measured subprocesses sequentially.
This controls interference from this program; unrelated host activity remains possible.
Use separate temporary directories so repeated operations do not reuse stale files.

The benchmark may alternate its internal procedures to reduce ordering effects.
Keep raw timing samples so views can report means, medians, and variability.
Variation across different coefficient sets is distinct from repeated-run noise
for one coefficient set.

Record failures with a short reason and relevant process status. Avoid an
automatic retry system initially. Explicit reruns append attempts; the view's
selection rule must avoid counting retries as independent timing samples.

Persist an attempt's start and finish so interrupted work is recognizable.
Resume checks existing records for this exact experiment and benchmark collection.
A build failure leaves the commit incomplete; a failed benchmark remains a visible
terminal outcome. No generic `Job`, `Variant`, or execution graph is needed.

## 9. Storage contract

```rust
trait Store: ReferenceCache {
    fn append<O: Serialize>(
        &mut self,
        record: &RunRecord<O>,
    ) -> Result<RecordId, StoreError>;

    fn history<O: DeserializeOwned>(
        &self,
        query: &HistoryQuery,
    ) -> Result<History<O>, StoreError>;

    fn last_complete(
        &self,
        experiment: ExperimentId,
    ) -> Result<Option<CommitId>, StoreError>;
}
```

This sketches the typed data operations. Experiment registration, attempt
start/finish, and commit-completion bookkeeping are additional lifecycle
operations on the same storage boundary, not new phase abstractions.

`RunRecord<O>` combines common metadata with a typed success output or explicit
failure/unsupported outcome. Metadata includes the experiment, commit, benchmark
kind/version, serialized benchmark configuration, execution time, and purpose.
The store assigns record IDs. Commit author, message, and timestamp are stored once
and can be joined into typed history results.

`HistoryQuery` filters common metadata: experiment, benchmark kind/version,
workload selection, commits, and attempt policy. Validate the stored output schema
before loading `History<O>`; do not mix unrelated payloads because a query is broad.
Support exact workload filtering first; arbitrary domain-field indexing can follow.

`SqliteStore` implements these traits using common metadata tables and versioned
serialized payloads. Benchmark authors do not write SQL. A test or in-memory
implementation can implement the same contracts.

Output-specific statistics need not be forced into a universal metric table in
the first version. The view can derive them from typed history. Add indexed metric
projections only when query size or frontend requirements justify them.

The generic methods intentionally use static dispatch. Do not assume this `Store`
can be placed behind `dyn Store` without a separate erased interface.

## 10. Typed views and graphical presentation

```rust
trait View {
    type Output: DeserializeOwned;
    type Selection;

    fn project(
        &self,
        history: &History<Self::Output>,
        selection: &Self::Selection,
    ) -> Result<ViewData, ViewError>;
}
```

`McmView` chooses `Output = McmOutput` and a selection type containing coefficient
filters, repetition summary, and comparison choices. The caller loads typed
history through `Store`, then passes it to the view.

`ViewData` is frontend-independent: named series and tables with units, commit
coordinates, optional values, and coverage information. Web and TUI renderers
consume it without understanding MCM-specific result types.

The MCM view can reproduce the script's statistics over time:

- Runtime and operation counts for Yosys and the paper tool.
- Mean runtime, timing variation, and runtime ratio.
- Synthesis area with and without MCM, their ratio, and percentage savings.
- Better/same/worse counts using the script's multiplier-first, add/sub-second
  comparison, rather than assuming one scalar metric expresses quality.

Preserve the distinction between a ratio of average runtimes and an average of
per-workload ratios. The example script uses the former. Its area savings are
computed per workload and then averaged. Make the selected calculation explicit.

Summarize repeated timings within each workload before aggregating workloads.
Compare matching workloads with valid denominators and report excluded coverage.
Missing depth, missing area, failed runs, and zero denominators remain absent or
explicitly unavailable. Shared reference IDs are not extra independent samples.

A standard synthesis view can be reused across synthesis suites. Custom
comparisons implement their own view; they do not have to implement a frontend.

## 11. Retention and inspection reruns

```rust
struct ArtifactPolicy {
    logs: bool,
    scripts: bool,
    netlists: bool,
}
```

Proposed default: all flags are false.

Always retain benchmark definitions, commit metadata, parsed results, individual
timing samples, cache-reference metadata, and outcome summaries. These are the
history itself. Capturing stdout temporarily for parsing is separate from saving
it permanently. Temporary parser inputs are removed after use.

Raw logs, scripts, and generated netlists are opt-in. Netlists are not required
fields of normal result types. A benchmark needing an intermediate file for its
next operation can create it temporarily without retaining it.

To inspect a result, rerun its exact commit and benchmark configuration with
selected outputs enabled. Link the diagnostic attempt to its source record and
exclude it from normal timing summaries. Generating additional output can affect
runtime, so it must not replace the historical measurement.

Reproduction requires the original input contents, tool, and benchmark version.
If they are unavailable or differ, report that limitation. An inspection rerun
produces a new artifact; it cannot recover the original run's discarded bytes.

One possible later refinement is retaining logs only for failures. That is not
part of the default here.

## Implementation order

- [ ] Agree on the `Benchmark`, `Suite`, `Builder`, `Store`, and `View` contracts.
- [ ] Port one coefficient-set comparison from the example script into an MCM
      benchmark with typed output and sequential timing samples.
- [ ] Generate the fixed coefficient collection through an MCM suite.
- [ ] Add the paper-result cache with explicit reference reuse.
- [ ] Add typed SQLite persistence and an MCM view for one commit.
- [ ] Add branch selection, commit-based Yosys builds, and the history loop.
- [ ] Add completion lookup, explicit reruns, and opt-in artifact inspection.
- [ ] Add a graphical renderer and a conventional design/library synthesis suite.

Useful checks include the same coefficients reaching both tools, one paper
reference reused across two Yosys commits, fresh Yosys timing samples at each
commit, a visible failed comparison, and an inspection rerun excluded from timing
history. No Rust implementation is part of this revision of the plan.

## Remaining review choices

1. Is one benchmark instance per coefficient set or design/library pair the right
   unit, with `Suite` owning the full sweep?
2. Is the proposed main-line branch traversal appropriate for merge commits?
3. Should a failed MCM phase fail the complete comparison, or should the output
   retain successful phases through per-field outcomes?
4. Should paper timing be reused across commits in the same environment, or
   refreshed per session while caching its structural result separately?
5. Are all raw outputs off by default, or should failures retain logs?
6. Should typed views be the initial query boundary, with metric indexing added
   later if needed?
