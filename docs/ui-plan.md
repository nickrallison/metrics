# Results graph demo

Status: implemented. Export with `view_results --html output/mcm/results.html`.

Build a single browser page centered on an interactive commit-history graph.
Generate it from saved results through `view_results`; keep projection math in
Rust. The first demo is an HTML snapshot that opens locally without a server.

## Appearance

Use the blue and pink branding of the Yosys Read the Docs site, which uses
[Furo-ys](https://yosyshq.net/furo-ys/). Use a white graph background, pale neutral
panels, dark readable text, thin grid lines, and restrained borders. Copy the
actual brand color tokens from Furo-ys during implementation, rather than guessing
their hex values. Keep colors in a few CSS variables.

The graph should occupy most of the window. Controls sit in one compact row
above it; point details sit below it. No dashboard of unrelated cards.

```text
MCM results                                      Experiment 3
Snapshot: <export time>                          <commit count> commits

Metric [Runtime ratio v]   Inputs [All inputs: aggregate v]
Coefficient set [#0: [493, -1587, ...] v]         (single-input mode)

Runtime ratio (Yosys / paper)              [Pan] [Zoom] [Reset] [Export]
  |
  |       o---o
1 | - - - - - - - - - - - - - - - - - - -  Equal runtime
  |              o---o     o---o
  |                   gap
  +---------------------------------------------------------
       fb1a2fd      7d3b4d2      ...              5797a7c
                           Commit order

Selected: <SHA>  <subject>  <value>  Inputs: 8/10
Excluded inputs: <expand for reasons>
```

Blue is the primary series and active control color. Pink identifies a second
comparison series, selected points, and interaction accents. When comparing
individual inputs, give each a stable color plus marker/dash pattern; selection
must remain understandable without color alone.

## Metrics and input projections

Keep metric choice separate from input selection. Switching either redraws the
same graph.

First-demo metrics reuse the existing `McmView` projections:

| Metric | Axis | Reference line |
|---|---|---|
| Runtime ratio | Yosys time / paper time; lower is better | 1: equal runtime |
| Area savings | Percent saved against synthesis without MCM; higher is better | 0%: equal area |

First-demo input modes:

| Mode | Behavior |
|---|---|
| All inputs: aggregate | One point per commit using all valid workloads |
| One coefficient set | One point per commit for the chosen frozen workload |
| Compare coefficient sets | Overlay chosen workloads as individually labeled lines |

Identify workloads by their original index, not their coefficient values: two
workloads may have identical coefficients. The picker shows the index and ordered
coefficient list, with full values available in its details. Include input width
and library in the selected workload's details.

Preserve the existing math: average repetitions within each workload first.
Aggregate runtime ratio is mean Yosys time divided by mean paper time on matched
workloads. Aggregate area savings is the mean of per-workload percentage savings.
Each workload has equal weight. Comparing selected workloads overlays their
individual projections; averaging an arbitrary selected subset can follow later.

## Mouse navigation

- Hover: show commit hash, subject, metric value and unit, and included/total inputs.
- Wheel or trackpad scroll over the plot: zoom around the pointer.
- Drag: pan. A toolbar button switches dragging to box zoom.
- Double-click or Reset: fit the whole history.
- Click a point: pin its details below the graph, including exclusion reasons.
- Click legend entries: hide/show input lines without changing the input selection.
- Export: save the visible graph as an image.

Keep the commit viewport when changing input projections. Reset the vertical
range when changing metrics, since their units differ. Controls need labels and
keyboard focus; Reset also offers an alternative to mouse gestures.

## Data behavior

- Keep the database's frozen commit order. Space commits equally; author dates
  can be misleading after rebases. Show dates as metadata rather than sorting by them.
- Use the latest finished measurement per commit/workload, matching the CLI.
  Do not pool retries or include diagnostic perf timings as samples.
- Missing, failed, unsupported, and invalid observations produce gaps, never zero.
  Do not connect lines across missing points. Give missing positions a neutral
  status marker so users can inspect their reason without a numeric value.
- Aggregates exclude invalid workloads, matching the CLI. Always display coverage;
  call out changing coverage so it cannot silently look like an improvement.
- If no valid points exist, show an informative empty state and the available
  exclusion reasons, with the input/metric controls still usable.
- Do not invent error bars when there is only one repetition.

The demo represents one experiment selected at export time. Its title includes
the experiment ID and snapshot time. Regenerate and reload to see new results;
live refresh and an in-page experiment picker can follow later.

## Small implementation

Command:

```sh
cargo run --bin view_results -- --experiment 3 --html output/mcm/results.html
```

Extend the existing binary. Retain its text and JSON modes. Bundle a pinned
[Plotly.js](https://plotly.com/javascript/configuration-options/) basic build
locally: its existing pan, zoom, hover, legend, and image-export controls cover
this demo. No Node build or network access should be needed to view the export.

Suggested files:

```text
src/view/html.rs                 Snapshot document and generic graph data
src/benchmarks/mcm/view.rs       Existing metric projection logic
src/bin/view_results.rs          HTML output option
ui/results.js                   Controls, graph rendering, point selection
ui/results.css                  Yosys theme and layout
ui/vendor/plotly-basic.min.js    Pinned dependency, with its license
```

Load typed history once, filter it by original workload index, and invoke
`McmView::project` for the aggregate and each individual input. Export these small
precomputed series plus commit/workload metadata. JavaScript selects and overlays
them; it does not reimplement the statistical formulas. Reuse `ViewData` for the
series. No storage schema changes are needed.

Embed data, scripts, and styles into the output HTML so the demo is one portable
file. Escape embedded JSON and insert commit messages/parameters as text.

## Implementation order and acceptance

1. Export real aggregate and individual-input series with commit metadata.
2. Add the graph, both selectors, and the Yosys theme.
3. Enable mouse navigation, pinned details, missing-data markers, and image export.
4. Check the demo against the CLI on real saved results.

The first demo is complete when both metrics match CLI values, every coefficient
set is selectable, comparisons overlay correctly, gaps and coverage remain
visible, and zoom/pan/reset work in a browser offline. Verify that metric changes
reset incompatible vertical scales and input changes retain the commit viewport.

Later additions: aggregate a selected subset, require a common valid input set
across compared commits, raw runtime/area comparisons, live refresh, experiment
selection, and links from individual results to their saved flamegraphs.
