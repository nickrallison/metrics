# Results UI

The Dioxus app owns page structure, controls, tables, and selected-point details.
It is benchmark independent: an analysis provider supplies serializable
descriptors, parameter options, chart series, and details. Typed benchmark
outputs stay in the server process.

The shared catalog database maps opaque experiment IDs to experiment-specific
SQLite databases and local experiment IDs. Running an experiment registers its
database; the web app also indexes existing databases under the configured
output root. Database paths and local storage IDs stay on the server.

Compiled benchmark modules register typed analysis adapters in one registry.
Each adapter queries a typed `Frame`, applies domain logic through `Analysis`,
then converts its output to the shared presentation model. The server filters
available descriptors for each catalog entry. Closures and stored output types
never cross the browser boundary.

The Plotly adapter is the only JavaScript written for the app. It accepts a
generic `ChartSpec`, supports pan, zoom, hover, and click events, and reports
selected series and point indices to Dioxus. Plotly is vendored with its license.
Furo-ys informs the CSS color and spacing tokens; the Sphinx theme itself is not
embedded in the Dioxus app.

Benchmark-specific adapters own their metric names and projections. Shared UI
components render descriptors and parameters without referring to any benchmark
or experiment type.

The CLI keeps text, JSON, detailed report, and attempt output. Portable HTML
export was removed so there is one web UI implementation and no parallel
`results.html`/`results.js` page.
