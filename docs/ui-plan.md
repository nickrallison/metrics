# Results UI

The Dioxus app owns page structure, controls, tables, and selected-point details.
It is benchmark independent: an analysis provider supplies serializable
descriptors, parameter options, chart series, and details. Typed benchmark
outputs stay in the server process.

The server registry dispatches stable analysis IDs to compiled Rust providers.
Each provider uses `Store::query` to load a typed `Frame`, applies domain logic
through `Analysis`, then converts its output to the shared presentation model.
Closures and stored output types never cross the browser boundary.

The Plotly adapter is the only JavaScript written for the app. It accepts a
generic `ChartSpec`, supports pan, zoom, hover, and click events, and reports
selected series and point indices to Dioxus. Plotly is vendored with its license.
Furo-ys informs the CSS color and spacing tokens; the Sphinx theme itself is not
embedded in the Dioxus app.

The first provider is MCM performance history. It exposes runtime ratio and area
savings as metric choices and aggregate or per-input lines as series choices.
Additional benchmark families can register providers without adding their
controls or labels to shared UI components.

The CLI keeps text, JSON, detailed report, and attempt output. Portable HTML
export was removed so there is one web UI implementation and no parallel
`results.html`/`results.js` page.
