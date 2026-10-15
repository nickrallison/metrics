# Generalized thoughts

- A lot of code written would be better moved into the `Benchmark` definition and impls as opposed to being defined in their actual types
  - E.G. SynthBenchmark could define
    - How to run tests
    - how to parse results
    - if it should run multiple times
- A portion of the below types would be better as traits with associated types
- I think the hierarchy is slightly wrong where multiple processes running happens on one commit
  - i.e. with MCM, without MCM, and the research tool which gives the optimzied answer, but no verilog

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

- Notes:

I think this abstraction is close, the nice path is that you just want to look at the history of a thing on a single branch

I don't know if thats ever not true, maybe lets assume it is for the sake of simplicity

Then revision selection can be Explicit, or it could be a Since(CommitId), which would select all commits since that point,

essentially it is just meant to be a collection representing different states of the tool to be tested, I don't think that First Parent Range is needed

Then a parent structure can specify a branch on which to work

---

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
    author: String
    commit_message: String,
}
```

- Notes:

If the DB can be searched for the prior commit (by whatever definition of prior),
then the `Since` can be resolved to an explicit list of commits. This should simplify the structure for getting commits from the repo

---

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

- Notes:

This section seems pretty wildly overcomplicated for the moment. at least for my use case, I think commit id is fine for identifying a build, other things can be added later like build flags.

---

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

- Notes:

I generally like this, not really sure what these lines are for:

`id: DesignId,` and `id: LibraryId,` or how they are constructed

I think Case might need some work, for the general case, the cases are the cartesian product of the designs and cell libs

for the MCM case, it was just the design set, but one of the profiled tools didn't take verilog, and just took a set of coefficients and output the adder / subtraction dag

The same coefficients were used to construct verilog designs for the MCM case which were then run with and without MCM

I think the abstraction level for verilog design is fine, however it probably cannot be the base level for everything

even though the tool was unchanging, the performance of yosys relative to it was changing, it can (should) be cached but accounted for

Otherwise, I think case is over concretized, it should probably be more generic with associated types, **this is probably true elsewhere too**

---

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

- notes:

again generally I like this,

the only issues i possibly foresee is that saving all netlists might possibly be too big. I think getting a netlist out should be done by a user afterwards, but probably shouldnt be done eagerly

---

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
    wall_clock_time: Duration
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

- Notes:

Again this seems overcomplicated, maybe refine it / simplify it,

Also the parameter sweep is necessarilly generic per experiment, but runs on a single commit

Also a note from elsewhere but I think having the option to record specific outputs would be useful, but have everything default to not, if someone wants to view it, they can rerun with some outputs enabled? More thought should be put here, I don't actually think I know what should be saved vs not

---

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

- Notes:

While necessary I think this is overengineered. It's necessary because to get useful runtime info, we need to run the benchmarks sequentially (to avoid interference from other processes). as well as possibly multiple times (to get a stable average).

otherwise, this should probably be simplified

---

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

- Notes:

I think this should define a trait or two that anything else can implement to add a record, or get a view

more traits, less concrete types

---

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

- Notes:

I don't know what this will look like without more generics, probably will need lots of changes.

---

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

# Example Use case

For Example, lets say I want a view of my MCM (Multiple Constant Multiplication) experiment over time

This was the script I ran and what inspired this project:

```python
import os
import sys
import re
import subprocess
import time
import statistics
import random

from enum import Enum
from textwrap import dedent

from pathlib import Path
from dataclasses import dataclass

COEFF_RE = re.compile(r"y\d+\s*=\s*x\s*\*\s*\((-?)\d+\'sd(\d+)\);")
STAT_RE = re.compile(r"^\s*(\d+)\s+\$(\w+)\s*$", re.MULTILINE)
AREA_RE = re.compile(r"^\s*Chip area for (?:top )?module '[^']+':\s*([\d.eE+-]+)\s*$", re.MULTILINE)
DEPTH_RE = re.compile(r"^\s*mcm: .* -> \d+ adder\(s\), depth (\d+),", re.MULTILINE)
ACM_COST_RE = re.compile(r"^// Cost: (\d+) adds/subtracts (\d+) shifts (\d+) negations$", re.MULTILINE)
ACM_DEPTH_RE = re.compile(r"^// Depth: (\d+)$", re.MULTILINE)

@dataclass
class ProfileResult:
	seconds: float
	adds_subs: int
	negations: int
	depth: int | None
	shifts: int | None = None
	mul: int = 0

	def time(self) -> str:
		return f"{self.seconds:.3f}s"

	def stats(self) -> str:
		depth = self.depth if self.depth is not None else "unknown"
		return f"add+sub={self.adds_subs}, mul={self.mul}, depth={depth}"

def parse_yosys(stdout: str, seconds: float) -> ProfileResult:
	if not re.search(r"^=== .+ ===$", stdout, re.MULTILINE):
		raise ValueError("Missing Yosys statistics")
	cells: dict[str, int] = {}
	for count, cell in STAT_RE.findall(stdout):
		cells[cell] = cells.get(cell, 0) + int(count)
	depths = [int(depth) for depth in DEPTH_RE.findall(stdout)]
	return ProfileResult(seconds, cells.get("add", 0) + cells.get("sub", 0), cells.get("neg", 0), max(depths, default=None), mul=cells.get("mul", 0))

def parse_acm(stdout: str, seconds: float) -> ProfileResult:
	cost = ACM_COST_RE.search(stdout)
	depth = ACM_DEPTH_RE.search(stdout)
	if cost is None or depth is None:
		raise ValueError("Missing ACM cost or depth")
	adds_subs, shifts, negations = map(int, cost.groups())
	mul = len(re.findall(r"^\s*t\d+\s*=\s*cmul\(", stdout, re.MULTILINE))
	return ProfileResult(seconds, adds_subs, negations, int(depth.group(1)), shifts, mul)

def run_profile(args: list[str], log_path: Path) -> tuple[str, float]:
	log_path.parent.mkdir(parents=True, exist_ok=True)
	start = time.perf_counter()
	res = subprocess.run(args, capture_output=True, text=True)
	seconds = time.perf_counter() - start
	log_path.write_text(res.stdout)
	res.check_returncode()
	return res.stdout, seconds


class Coeff:
	def __init__(self, number: int, bit_width: int):
		self.number = number
		self.bit_width = bit_width

	def __str__(self):
		return f"{self.number}"

class Test:
	def __init__(self, input_width: int, coeffs: list[Coeff], idx: int, output_dir: Path):
		self.coeffs = coeffs
		self.input_width = input_width
		self.results_dir = output_dir / f"test_{idx}"
		self.verilog_file = self.results_dir / f"unoptimized.v"

		self.generate_v_file()

	def generate_v_file(self) -> None:
		self.results_dir.mkdir(parents=True, exist_ok=True)


		file = "\n".join([
		    "module unoptimized(",
		    f"    input wire signed [{self.input_width - 1}:0] x,",
		    ",\n".join(
		        f"    output wire signed [{c.bit_width+self.input_width-1}:0] y{i}"
		        for i, c in enumerate(self.coeffs)
		    ),
		    ");",
		    "\n".join(
				f"    assign y{i} = x * ({'-' if c.number < 0 else ''}{self.input_width-1}'sd{abs(c.number)});"
		        for i, c in enumerate(self.coeffs)
		    ),
		    "endmodule",
		    "",
		])

		with open(self.verilog_file, "w") as f:
			f.write(file)

	def profile_yosys(self, yosys_path: Path) -> ProfileResult:
		verilog_path = self.results_dir / "yosys.v"
		stdout, seconds = run_profile(
			[str(yosys_path), "-p", f'read_verilog "{self.verilog_file}"; mcm -depth 16 -search_budget 200000000; stat; write_verilog "{verilog_path}"'],
			self.results_dir / "yosys.log",
		)
		return parse_yosys(stdout, seconds)

	def profile_acm(self, acm_path: Path) -> ProfileResult:
		coeffs = [str(coeff) for coeff in self.coeffs]
		args = [str(acm_path), "-b", "30", *coeffs, "-gc", "-seed", "1"]
		stdout, seconds = run_profile(args, self.results_dir / "acm.log")
		return parse_acm(stdout, seconds)

	def profile_synth(self, yosys_path: Path, liberty_path: Path, use_mcm: bool) -> float:
		name = "synth_mcm" if use_mcm else "synth"
		script_path = self.results_dir / f"{name}.ys"
		stats_path = self.results_dir / f"{name}.stat"
		mcm = "mcm -depth 16 -search_budget 200000000\n" if use_mcm else ""
		script_path.write_text(
			f'read_verilog "{self.verilog_file}"\n'
			"hierarchy -top unoptimized\n"
			+ mcm + dedent(f"""\
				synth -top unoptimized -noabc
				abc -liberty "{liberty_path}"
				tee -o "{stats_path}" stat -liberty "{liberty_path}"
			""")
		)
		run_profile([str(yosys_path), "-s", str(script_path)], self.results_dir / f"{name}.log")
		area = AREA_RE.search(stats_path.read_text())
		if area is None:
			raise ValueError(f"Missing chip area in {stats_path}")
		return float(area.group(1))

	def profile(self, yosys_path: Path, acm_path: Path, liberty_path: Path, idx: int):
		yosys = self.profile_yosys(yosys_path)
		acm = self.profile_acm(acm_path)
		synth_area = self.profile_synth(yosys_path, liberty_path, use_mcm=False)
		mcm_area = self.profile_synth(yosys_path, liberty_path, use_mcm=True)
		relative_area = f"{mcm_area / synth_area:.2%}" if synth_area else "n/a"
		coeffs = [str(coeff) for coeff in self.coeffs]
		ratio = yosys.seconds / acm.seconds
		items = [
			("yosys runtime", yosys.time()),
			("hcub_paper runtime", acm.time()),
			("yosys stats", yosys.stats()),
			("hcub_paper stats", acm.stats()),
			("synth area", f"{synth_area:.3f}"),
			("synth with MCM area", f"{mcm_area:.3f}"),
			("MCM / baseline area", relative_area),
			("runtime ratio", f"{ratio:.2f}"),
			("coeffs", ", ".join(coeffs)),
		]
		print_table(f"Test {idx}", items)
		return yosys, acm, synth_area, mcm_area

def print_table(title: str, items: list[tuple[str, str]]) -> None:
	print(title)
	longest_name = 0
	for name, value in items:
		longest_name = max(longest_name, len(name))
	for name, value in items:
		num_spaces = longest_name - len(name)
		value = str(value).replace("\n", "\n\t" + " " * (longest_name + 2))
		print(f"\t{name}:{' ' * num_spaces} {value}")
	print()

class Cmp(Enum):
		Better = 1
		Same = 2
		Worse = 3

def cmp(yosys: ProfileResult, acm: ProfileResult) -> Cmp:
	if yosys.mul > acm.mul:
		return Cmp.Worse
	elif yosys.mul < acm.mul:
		return Cmp.Better

	if yosys.adds_subs > acm.adds_subs:
		return Cmp.Worse
	elif yosys.adds_subs < acm.adds_subs:
		return Cmp.Better

	return Cmp.Same

def bit_width(val: int):
	return len(bin(val)) - 2

def generate_coeffs(rng: random.Random, number_tests: int, length_range: tuple[int, int], value_range: tuple[int, int], seed=42) -> list[list[Coeff]]:
	rng.seed(seed)
	coeffs = [sample_coeff(rng, length_range, value_range) for idx in range(number_tests)]
	return coeffs

# smaller a means proportially more larger values and fewer smaller values
# larger scale shifts all values upwards
# the distrition is the following
# p(x) ~ 1 - x^(-a)
def sample_coeff(
    rng: random.Random,
    length_range: tuple[int, int],
    value_range: tuple[int, int],
    a: float = 1.3,
    scale: float = 1024,
) -> list[Coeff]:
    if a <= 1 or scale <= 0:
        raise ValueError("Require a > 1 and scale > 0")

    length = rng.randint(*length_range)
    low, high = value_range
    coeffs = []

    while len(coeffs) < length:
        magnitude = int(scale * (rng.paretovariate(a - 1) - 1))
        val = rng.choice((-1, 1)) * magnitude
        if low <= val <= high:
            coeffs.append(Coeff(val, bit_width(val)))

    return coeffs

def main():
	if not "YOSYS" in os.environ:
		raise Exception("Set YOSYS environment variable to absolute path of yosys binary")

	yosys_bin = Path(os.environ["YOSYS"])
	root_dir = Path(sys.argv[0]).parent
	acm_path = root_dir / "synth" / "acm"
	liberty_path = root_dir / "NangateOpenCellLibrary_typical.lib"
	output_dir = root_dir / "results"

	number_tests = 40
	seed = 42
	rng = random.Random()

	input_bit_width = 32
	length_range = (1, 6)
	value_range = (-2_147_483_648, 2_147_483_647)

	coeffs = generate_coeffs(rng, number_tests, length_range, value_range, seed)
	results = []
	area_results = []

	for idx, coeff in enumerate(coeffs):
		test_file = Test(input_bit_width, coeff, idx, output_dir)
		yosys_res, acm_res, synth_area, mcm_area = test_file.profile(yosys_bin, acm_path, liberty_path, idx)
		results.append((yosys_res, acm_res))
		area_results.append((synth_area, mcm_area))

	# 1000ms = 1s
	timescale = 1000

	avg_yosys_runtime = sum(res.seconds * timescale for res, _ in results) / len(results)
	avg_acm_runtime = sum(res.seconds * timescale for _, res in results) / len(results)
	stdev_yosys_runtime = statistics.stdev([res.seconds * timescale for res, _ in results])
	stdev_acm_runtime = statistics.stdev([res.seconds * timescale for _, res in results])

	avg_runtime_ratio = avg_yosys_runtime / avg_acm_runtime

	compared_results = [cmp(yosys_res, acm_res) for (yosys_res, acm_res) in results]

	num_better = len([i for i in compared_results if i == Cmp.Better])
	num_same   = len([i for i in compared_results if i == Cmp.Same])
	num_worse  = len([i for i in compared_results if i == Cmp.Worse])
	num_mcm_better_or_same = sum(mcm_area <= synth_area for synth_area, mcm_area in area_results)
	num_mcm_larger = len(area_results) - num_mcm_better_or_same
	area_savings = [100 * (1 - mcm_area / synth_area) for synth_area, mcm_area in area_results if synth_area]
	avg_area_savings = f"{statistics.mean(area_savings):.2f}%" if area_savings else "n/a"

	items = [
		("Avg Yosys runtime (ms)", f"{avg_yosys_runtime:.2f}"),
		("Avg hcub_paper runtime (ms)", f"{avg_acm_runtime:.2f}"),
		("Stdev Yosys runtime (ms)", f"{stdev_yosys_runtime:.2f}"),
		("Stdev hcub_paper runtime (ms)", f"{stdev_acm_runtime:.2f}"),
		("Average hcub_paper Speed Up", f"{avg_runtime_ratio:.2f}"),
		("Number of Times Yosys Results Smaller v.s. HCUB Paper", str(num_better)),
		("Number of Times Yosys Results Same v.s. HCUB Paper", str(num_same)),
		("Number of Times Yosys Results Larger v.s. HCUB Paper", str(num_worse)),
		("Number of Times MCM Area Smaller or Same v.s. Without MCM", str(num_mcm_better_or_same)),
		("Number of Times MCM Area Larger v.s. Without MCM", str(num_mcm_larger)),
		("Avg MCM area savings", avg_area_savings),
	]
	print_table("Results", items)

if __name__ == "__main__":
	main()
```

This is essentially what I want except a graphical view over time

So the user could define

```rust
struct McmBenchmark {
    coefficients: Vec<u32>,
}

impl Benchmark for McmBenchmark {
    type Input = ...; // yosys path, paper tool path, ...
    type Output = McmBenchmarkOut;

    fn run(&self, input: Self::Input) -> Self::Output {

    }
}

// duration, area, cellcount, specific cell count...
struct YosysStats;

// duration, adders, subtracters, any remaining multipliers
struct PaperStats;



struct McmBenchmarkOut {
    yosys_mcm_stats: YosysStats,
    yosys_no_mcm_stats: YosysStats,
    paper_stats: PaperStats,
}

//



```
