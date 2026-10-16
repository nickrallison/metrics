# MCM reference experiment

[mcm.py](mcm.py) is an unchanged copy of the user-provided prototype reviewed at
commit `ab56896da386e9f70929739a28df69bcd4062c41`. It preserves the exact command
sequences, parsers, workload generator, and original summary calculations.

This is a reference for implementing the Rust benchmark. The Rust examples do
not invoke it. It requires a Yosys executable supporting `mcm`, the paper-tool
executable, and the Liberty file. For standalone use, the script expects:

```text
<script directory>/
  mcm.py
  synth/acm
  NangateOpenCellLibrary_typical.lib
  results/                         created by the script
```

Set `YOSYS` to the absolute executable path. The paper tool and library are not
bundled here. Running the script launches real tools and writes logs, scripts,
generated Verilog, and netlists under its `results/` directory.

## Workloads and operations

The prototype generates 40 ordered coefficient sets with seed 42, input width
32, set lengths from 1 through 6, and coefficient values in
`[-2_147_483_648, 2_147_483_647]`. Each magnitude is the truncated value of
`1024 * (paretovariate(0.3) - 1)`, with a random sign and rejection outside the
value range. Duplicates and zero are not excluded.

Preserve generated values in the Rust experiment. Using seed 42 with a different
language/RNG does not guarantee the same collection.

For each set, the prototype:

1. Generates signed Verilog multiplications under top module `unoptimized`.
2. Runs `read_verilog; mcm -depth 16 -search_budget 200000000; stat;
   write_verilog` in Yosys.
3. Runs the paper tool with `-b 30 <coefficients> -gc -seed 1`. Its output describes
   the optimized arithmetic graph, not a Verilog design.
4. Runs mapped synthesis without MCM: `read_verilog`, `hierarchy`,
   `synth -top unoptimized -noabc`, `abc -liberty`, then `stat -liberty` via `tee`.
5. Runs the same synthesis sequence with the MCM pass inserted before `synth`.

The full commands and filename handling are in the script. In particular, this
historical MCM synthesis flow does not contain a `dfflibmap` command; do not
silently substitute a different synthesis recipe when porting the comparison.

## Parser behavior

- Yosys parsing requires a statistics header, sums matched `$<cell>` counts, and
  reports add+sub, negations, and remaining multipliers. Missing cell categories
  default to zero. Depth is the maximum matched MCM depth, or absent.
- Paper parsing requires the `// Cost:` and `// Depth:` lines. It extracts
  add/sub, shift, and negation counts and counts `cmul` expressions as remaining
  multipliers.
- Area parsing requires a matching `Chip area for ... module` line. Missing area
  is a parser error in the prototype; the Rust result type can represent absence.
- Process failure is checked before parsing. Historical text formats may need
  explicit adapters or unsupported outcomes.

The Yosys parser sums all matching cell-table lines in stdout. Verify module and
statistics-block scope when implementing a parser for other output shapes.

The Verilog generator also preserves historical sizing choices: coefficient
`bit_width` is calculated using `len(bin(value)) - 2`, which includes an extra
character for negative values, while literal width is `input_width - 1`.
The Rust implementation uses MCM procedure version 2: coefficient literals have
a sign bit plus all magnitude bits, and output widths are the sum of the operand
widths. This fixes truncation/sign interpretation while preserving coefficient
values and order. Version 1 experiments remain separate and readable.

## Statistics and differences from the Rust design

- Runtime ratio is average Yosys runtime divided by average paper runtime, using
  matched workloads. This differs from averaging each workload's runtime ratio.
- Area savings is the mean of `100 * (1 - mcm_area / baseline_area)` over workloads
  with a nonzero baseline. Per-workload area ratio is `mcm_area / baseline_area`.
- Quality compares remaining multipliers first, then add/sub counts. Lower is
  better; equal values are reported as the same. Depth and negations do not enter
  this ordering. Area comparisons separately count MCM area <= baseline area.
- The prototype runs each phase once per workload. Its sample standard deviations
  are across workloads, not estimates of repeated-run noise. Rust should retain
  repeated samples and distinguish these two sources of variation.
- The prototype reruns the paper tool for every workload, eagerly retains files,
  and includes `write_verilog` in one timed command. Rust defaults to no permanent
  raw outputs and may cache independent paper observations. Diagnostic output
  generation changes the timing scope; record it as an inspection attempt.
- The prototype's subprocess timer covers invocation/output collection, with log
  writing afterward. Preparation and parsing are outside that timing interval.
- The Rust view must handle missing/failed measurements and invalid denominators
  explicitly, and report excluded coverage. The prototype has fewer guards.

See [the implementation guide](../../docs/scaffold.md) and the Rust module docs
for the current contracts and remaining decisions.
