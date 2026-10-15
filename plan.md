# Yosys benchmark history plan

Status: proposal for review. Names and boundaries are provisional.

This document describes types and responsibilities, not an implementation. Rust
sketches omit imports, derives, visibility, identifier wrappers, and supporting
error types. No dependency choices are final.

## Goal

Measure how Yosys synthesis characteristics change across commits while holding
the selected workloads and synthesis procedures fixed.

The initial workflow is:

1. Select Yosys revisions, designs, libraries, and procedure configurations.
2. Prepare workloads and build the selected Yosys revisions.
3. Execute valid combinations, optionally with repeated measurements.
4. Store results, failures, logs, and provenance.
5. Query a metric's history for selected workloads and configurations.

Yosys revision is the primary variable under test. Cell counts, mapped area,
runtime, and other synthesis statistics are observations. Build settings,
dependencies, design contents, libraries, and procedure options are recorded
because they can affect those observations.

The first version supports one benchmark implementation type per experiment,
with multiple configurations of that implementation. OpenROAD, unrelated tools,
and mixed benchmark types remain possible extensions.

## Vocabulary and ownership

| Type            | Meaning                                                             |
| --------------- | ------------------------------------------------------------------- |
| `Benchmark`     | A user-defined procedure and its result interpretation              |
| `Case<I>`       | One named, identifiable input with searchable dimensions            |
| `Suite<I>`      | A collection of cases sharing an input type                         |
| `Variant<B>`    | A named benchmark configuration and revision/build selection        |
| `Experiment<B>` | The requested comparison, including suite and execution policy      |
| `Plan<B>`       | The resolved cases, variants, builds, and jobs                      |
| `Job`           | One intended measurement for a case, variant, build, and repetition |
| `RunRecord<O>`  | One actual execution attempt, including its outcome                 |

Use structs and enums for the surrounding phases. Start with `Benchmark` as the
main extension trait. Add other traits when multiple implementations establish
a useful common contract.

```text
Experiment<B>
    Suite<B::Input>
        Case<B::Input>
    Variant<B>
    RunPolicy
        |
        v
Planner -> Plan<B>
               YosysBuildSpec
               Job
                |
                v
Executor + YosysBuilder + Benchmark
                |
                v
RunRecord<B::Output> -> SqliteStore -> HistoryQuery -> TUI / web UI
```

## 1. Revision selection

```rust
enum RevisionSelection {
    Explicit(Vec<CommitId>),
    FirstParentRange {
        base: String,
        head: String,
    },
}

struct Commit {
    id: CommitId,
    parents: Vec<CommitId>,
    committed_at: Timestamp,
    subject: String,
}
```

A concrete `RevisionResolver` operates on a Git repository and returns ordered,
immutable commit descriptions.

- Resolve branch names and references before execution.
- Preserve the original selection as experiment provenance.
- Record range endpoint semantics explicitly; proposed default is base-exclusive,
  head-inclusive, ordered oldest to newest along the head's first-parent history.
- Reject a first-parent range whose base is not on that history.
- Preserve explicit-list order and reject duplicate revisions within that list.
- Keep revision ordering separate from commit timestamps and execution timestamps.

The same commit may appear in several variants. Build specifications can still
be deduplicated across those variants.

## 2. Build specification and acquisition

```rust
struct BuildConfig {
    recipe: BuildRecipeId,
    compiler: CompilerSpec,
    options: BTreeMap<String, String>,
    dependencies: DependencyPolicy,
}

struct YosysBuildSpec {
    id: BuildId,
    repository: RepositoryId,
    commit: Commit,
    config: BuildConfig,
}

struct YosysBuilder {
    cache: BuildCache,
    workspace_root: PathBuf,
}

struct YosysBuild {
    id: BuildId,
    executable: PathBuf,
    data_dir: PathBuf,
    provenance: BuildProvenance,
}

struct BuildProvenance {
    spec: YosysBuildSpec,
    environment: BuildEnvironment,
    dependencies: Vec<ResolvedDependency>,
}
```

`YosysBuilder::ensure` takes a specification and returns a validated cached or
newly built installation. It owns isolated checkouts, compilation, build logs,
and cache validation. Keep installation fields private so arbitrary paths do not
implicitly qualify as usable Yosys builds.

`DependencyPolicy` distinguishes using dependencies associated with each Yosys
revision from pinning dependencies across revisions. Record actual dependency
identities, including ABC where applicable. These policies answer different
experimental questions.

The build cache must account for the recipe, effective compiler/toolchain,
settings, relevant environment, and resolved dependencies. If dependencies are
resolved during acquisition, finalize the cache identity after resolution rather
than trusting an unresolved specification alone.

Build outcomes have separate records. A failed build blocks its dependent jobs;
those jobs did not fail during synthesis. Retain build logs and provenance even
when no usable installation is produced.

Build time is outside synthesis measurements.

## 3. Workload preparation and suites

```rust
struct VerilogDesign {
    id: DesignId,
    top: String,
    sources: Vec<SourceFile>,
    include_dirs: Vec<PathBuf>,
    defines: BTreeMap<String, Option<String>>,
    fingerprint: ContentHash,
}

struct CellLibrary {
    id: LibraryId,
    liberty: PathBuf,
    fingerprint: ContentHash,
}

struct SynthInput {
    design: Arc<VerilogDesign>,
    library: Arc<CellLibrary>,
}

struct Case<I> {
    id: CaseId,
    label: String,
    dimensions: BTreeMap<String, DimensionValue>,
    input: I,
}

struct Suite<I> {
    name: String,
    cases: Vec<Case<I>>,
}
```

`SourceFile` records the source path, language, and content identity. Source order,
top module, defines, included files, and other effective dependencies contribute
to the workload fingerprint. A directory path alone does not identify its contents.

A concrete `DesignPreparer` turns file descriptions or generator specifications
into prepared designs. Manifest loading and parameter expansion construct the
suite. The existing mux manifest is a candidate input format, rather than the
framework's internal representation.

```text
Source manifest -> parameter expansion -> preparation -> Suite<SynthInput>
```

Preparation records generator sources, parameters, dependencies, and resulting
content identities. Cache it independently from Yosys builds. Initially, an
external script emitting a concrete output manifest is sufficient.

Preparation is outside the measured procedure unless preparation performance is
explicitly the benchmark subject. It does not need to run during Cargo compilation.

The benchmark borrows prepared inputs. `Arc` allows shared designs and libraries
without carrying borrowed lifetimes through plans and stored records.

Do not require a universal `Design::into_verilog` conversion. A synthesis
benchmark can require `VerilogDesign`; conversions from other representations
are explicit preparation steps.

Cases carry searchable dimensions such as design family, generator parameters,
and library. Labels remain readable; fingerprints establish what actually ran.
Changed workload contents must remain distinguishable even when labels match.

## 4. Benchmark definition

```rust
trait Benchmark: Serialize {
    type Input: Serialize;
    type Output: Serialize;

    fn identity(&self) -> BenchmarkIdentity;

    fn run(
        &self,
        yosys: &YosysBuild,
        input: &Self::Input,
        context: &mut RunContext,
    ) -> Result<Self::Output, BenchmarkError>;

    fn metric_definitions(&self) -> Vec<MetricDefinition>;

    fn metrics(
        &self,
        output: &Self::Output,
    ) -> Vec<MetricValue>;
}

struct MappedSynth {
    options: SynthOptions,
}

struct SynthOutput {
    cell_count: u64,
    cells_by_type: BTreeMap<String, u64>,
    area: Option<f64>,
    netlist: Option<ArtifactRef>,
}
```

`MappedSynth` implements `Benchmark` with `Input = SynthInput` and
`Output = SynthOutput`. Associated types bind the procedure to its input/output
contract. Yosys is explicit in the initial interface.

`SynthOptions` records procedure choices: passes, arguments, seeds, and optional
artifact generation. Different option values can be compared as variants while
sharing the same case. Every effective execution option must be recorded.

`BenchmarkIdentity` provides a stable identifier, procedure version, and payload
schema information. Serialize the benchmark value to preserve its configuration.
Use stable identifiers rather than Rust type names as persistent identities.

`RunContext` provides an isolated workspace, recorded process launching,
cancellation, timeouts, and artifact registration. Launch subprocesses through
this context so their records survive subsequent parsing errors.

Framework records own invocation details, process elapsed time, and termination
status. Benchmark output owns parsed domain statistics. A benchmark does not
receive the database or scheduler.

Metric projection makes selected typed fields available to generic queries. For
example, cell counts are integer metrics; unavailable area remains absent.
Area units must reflect the library/tool's actual reporting convention.

Start with execution and parsing in one contract. Retain raw logs and generated
scripts so separate parsing or reparsing can be introduced later. Preserve parser
version and result history when interpretations change.

## 5. Experiment planning

```rust
struct Experiment<B: Benchmark> {
    repository: RepositoryId,
    suite: Suite<B::Input>,
    variants: Vec<Variant<B>>,
    policy: RunPolicy,
}

struct Variant<B> {
    name: String,
    revisions: RevisionSelection,
    build: BuildConfig,
    benchmark: B,
}

struct Plan<B: Benchmark> {
    id: ExperimentId,
    cases: Vec<Case<B::Input>>,
    variants: Vec<PlannedVariant<B>>,
    builds: Vec<YosysBuildSpec>,
    jobs: Vec<Job>,
    policy: RunPolicy,
    revision_order: Vec<VariantRevision>,
}

struct PlannedVariant<B> {
    id: VariantId,
    name: String,
    benchmark: B,
}

struct VariantRevision {
    variant: VariantId,
    build: BuildId,
    position: u64,
}

struct Job {
    id: JobId,
    case: CaseId,
    variant: VariantId,
    build: BuildId,
    repetition: u32,
}
```

A concrete `Planner` has a planning operation generic over `B`. It resolves
revisions, validates selections, expands allowed combinations, deduplicates builds,
and produces jobs. Preserve the original experiment description alongside the plan.

The experiment expresses selections; the plan fixes execution identities.
`VariantRevision` preserves ordering within each variant without assuming one
global order across branches.

Example variants:

| Variant     | Revisions                | Procedure               |
| ----------- | ------------------------ | ----------------------- |
| Baseline    | One fixed main commit    | Existing synthesis flow |
| Development | Development branch range | New pass enabled        |
| Control     | Same development range   | New pass disabled       |

All three can share a `Suite<SynthInput>`. Each job refers to one case, one variant,
one build, and one intended repetition.

Use explicit selection and planning predicates for restricted combinations.
Record exclusions and reasons so reduced coverage is visible. Checks requiring a
built executable can occur during execution and produce an unsupported outcome.

`RunPolicy` includes repetition count, warmup policy, timeout, ordering, and
resource/concurrency limits. Warmups are distinguishable from measured samples.
The exact policy fields remain a review decision.

Planning remains typed: `Plan<MappedSynth>` contains cases of `SynthInput`.
Persisted plans use versioned payloads; reloading a typed plan requires matching
deserialization support and benchmark identity validation.

## 6. Execution and failure recording

```rust
struct Executor {
    builder: YosysBuilder,
    workspaces: WorkspaceManager,
    environment: ExecutionEnvironment,
}

struct RunRecord<O> {
    id: RunId,
    job: JobId,
    started_at: Timestamp,
    finished_at: Timestamp,
    environment: EnvironmentId,
    processes: Vec<ProcessRecord>,
    artifacts: Vec<ArtifactRef>,
    outcome: RunOutcome<O>,
}

enum RunOutcome<O> {
    Succeeded(O),
    Failed(RunFailure),
    Unsupported(UnsupportedReason),
}

struct ProcessRecord {
    invocation: ProcessInvocation,
    termination: ProcessTermination,
    elapsed: Duration,
    stdout: ArtifactRef,
    stderr: ArtifactRef,
}
```

For each job, the executor obtains the build, checks compatibility, creates a
workspace, invokes the benchmark, collects context records, and persists the
attempt. The executor is concrete; its execution operation is generic over `B`.

`ProcessTermination` distinguishes normal exit, signal termination, timeout, and
launch failure. `RunFailure` distinguishes execution, parsing, and other benchmark
errors. Cancellation is recorded explicitly rather than presented as success.

Persist an attempt's start before launching work, then finalize its outcome.
Interrupted attempts must remain discoverable during resume. They can be tracked
as execution metadata before a complete `RunRecord` exists.

Repetitions and retries differ: a repetition is another intended measurement;
a retry is another attempt at the same job. Preserve all attempts and use an
explicit selection rule when summarizing retries, so they are not counted as
independent repetitions.

Build caching may reuse installations. Resuming may skip completed jobs. Requesting
another measurement must produce a new run rather than reuse a historical timing.

Start with external process wall time and retain individual samples. Define its
scope separately from complete-flow time or internal Yosys pass timings. Record
machine details, concurrency, and actual execution order. Alternating or
randomizing comparable variants can reduce order-related timing effects.

## 7. Persistence and metrics

```rust
struct SqliteStore {
    connection: Connection,
    artifacts: ArtifactStore,
}

struct MetricDefinition {
    id: MetricId,
    label: String,
    unit: Unit,
    preference: MetricPreference,
}

struct MetricValue {
    metric: MetricId,
    value: NumericValue,
}
```

`NumericValue` preserves integer and floating-point values. `MetricPreference`
expresses lower-is-better, higher-is-better, or neutral. Metric definitions also
need documented scope; metrics with different semantics must remain distinct.

The store's operations persist plans, build records, attempts, and metric values,
then answer result queries. Individual operations can be generic over payloads;
the store itself does not need a benchmark type parameter.

Use relational records for common fields and versioned structured payloads for
benchmark-specific configurations, inputs, and outputs.

| Common records                     | Purpose                                        |
| ---------------------------------- | ---------------------------------------------- |
| Experiments, cases, variants, jobs | Requested work and resolved selections         |
| Builds and build attempts          | Tool identities, provenance, failures          |
| Runs and processes                 | Execution history, status, timing, environment |
| Metric definitions and values      | Generic filtering, comparison, plotting        |
| Artifact references                | Logs, scripts, generated netlists, other files |

Keep large artifacts outside SQLite with managed references. Result finalization
must ensure referenced artifacts exist and related database rows are consistent.

Benchmark authors supply serializable types and metric projection. They do not
write SQL or per-benchmark table migrations. Deserialization bounds belong on
operations that actually load typed payloads.

No generic `Store` trait is required for a single SQLite backend.

## 8. Queries and presentation

```rust
struct HistoryQuery {
    experiment: ExperimentId,
    metric: MetricId,
    filters: Vec<DimensionFilter>,
    group_by: Vec<DimensionKey>,
    repetitions: RepeatSummary,
}

struct HistorySeries {
    dimensions: BTreeMap<String, DimensionValue>,
    points: Vec<HistoryPoint>,
}

struct HistoryPoint {
    build: BuildId,
    commit: CommitId,
    revision_order: u64,
    value: Option<NumericValue>,
    successful_runs: u32,
    failed_runs: u32,
}
```

The default grouping retains case and variant identity, with one point per build.
Summarize repetitions only after selecting the attempts eligible for analysis.
Unsupported and blocked coverage should also be available to the frontend, even
though the point sketch only shows success/failure counts.

Both a TUI and a web UI consume common query results. They need dimensions,
metric metadata, values, and coverage, rather than knowledge of `SynthOutput`.
`Display` can supply labels but is insufficient as a presentation contract.

Keep missing values absent. Do not treat failed or unsupported results as zero.
Retain distributions or sample access when summaries would hide timing variability.

Cross-case aggregation is an explicit additional operation. Raw area averages and
normalized ratios answer different questions. Comparisons must use matching
workloads and report missing coverage; changing suite contents must not silently
change the meaning of a history series.

## Identity rules

| Identity  | Must distinguish                                                      |
| --------- | --------------------------------------------------------------------- |
| Workload  | Effective design contents, library contents, input settings           |
| Build     | Yosys source, recipe, toolchain, settings, resolved dependencies      |
| Procedure | Benchmark identity/version and serialized configuration               |
| Job       | Case, procedure variant, build, repetition, relevant execution policy |
| Run       | Every actual attempt, including retries                               |

Readable labels are separate from these identities. Absolute paths and mutable
branch names are locations or selectors, not content identities. Use a defined,
versioned canonical encoding for content-derived identifiers rather than an
implementation-dependent Rust hash.

Preserve enough metadata to explain differences. Matching a label does not prove
two results are comparable. Procedure versions, build configurations, machine
environments, and workload changes need visible comparison boundaries.

## Extension boundaries

- Multiple tools: introduce explicit toolchain types when a concrete second tool
  requires them. The current benchmark interface intentionally accepts Yosys.
- Chained flows: first compose ordinary operations inside one benchmark. Pass
  typed artifacts carrying required metadata, not just paths.
- Shared stage abstractions: introduce them after repeated composition patterns
  emerge. Make intermediate caching and timing scope explicit.
- Mixed benchmark types: add a small enum or job adapter at the scheduling
  boundary. Keep typed execution within each benchmark implementation.
- External reference tools: extend tool acquisition and experiment composition
  when needed; they are not represented by the initial Yosys-only interface.
- Separate parsers: add versioned interpretation of saved outputs when reparsing
  or historical format support justifies the boundary.

## Suggested implementation order

- [ ] Agree on vocabulary, benchmark contract, and identity boundaries.
- [ ] Model prepared synthesis inputs and implement one mapped-synthesis benchmark.
- [ ] Record processes, failures, scripts, logs, and parsed statistics for one run.
- [ ] Resolve revision selections and acquire cached Yosys builds.
- [ ] Expand experiments into jobs and execute them with explicit repetition policy.
- [ ] Persist plans, build outcomes, runs, and metric projections in SQLite.
- [ ] Add resume behavior and historical-format compatibility handling.
- [ ] Return history series through a query interface.
- [ ] Build the first presentation frontend after the query contract is usable.

Each phase should be inspectable before adding the next. Useful initial checks
include executing the same case on two revisions, preserving a failed run,
distinguishing configurations of one commit, and resuming without counting retries
as additional planned measurements.

## Questions for review

1. Is a Yosys-specific benchmark interface the right initial boundary, or is a
   separate-tool comparison required in the first version?
2. Should procedure options live entirely in benchmark variants, or do some need
   to vary within suite inputs?
3. Is one benchmark implementation type per experiment sufficient initially?
4. Which dependency policy should be the default, particularly for ABC?
5. Are first-parent ranges and explicit commit lists enough for initial revision
   selection? Are the proposed range endpoint semantics appropriate?
6. Which manifest/preparation workflow should be supported first?
7. Which measurements, repetition policy, and timing scope are required first?
8. Should historical commits require one exact procedure, or may recorded
   compatibility adaptations preserve the intended procedure across versions?
9. Which frontend should come first once history queries work?

## Reference

The associated-type choice follows the distinction between implementer-selected
types and trait parameters in the [Rust book's advanced traits chapter](https://doc.rust-lang.org/book/ch20-02-advanced-traits.html#defining-traits-with-associated-types).
