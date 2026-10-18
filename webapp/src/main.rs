use dioxus::prelude::*;

fn main() {
    #[cfg(feature = "server")]
    dioxus::serve(|| async move { Ok(dioxus::server::router(App)) });

    #[cfg(not(feature = "server"))]
    dioxus::launch(App);
}

#[server]
async fn load_latest_snapshot() -> Result<String, ServerFnError> {
    use std::{collections::BTreeSet, path::PathBuf};

    use benchmarks::{
        benchmarks::mcm::{self, McmMetric, McmOutput, McmView},
        store::{HistoryQuery, SqliteStore, Store},
    };

    let database = std::env::var_os("BENCHMARKS_DATABASE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../output/mcm/database.sqlite")
        });
    if !database.is_file() {
        return Err(ServerFnError::new(format!(
            "Database {} does not exist. Set BENCHMARKS_DATABASE to the results database.",
            database.display()
        )));
    }

    let store = SqliteStore::open(database).map_err(server_error)?;
    let experiment = store
        .latest_experiment()
        .map_err(server_error)?
        .ok_or_else(|| ServerFnError::new("No experiments are recorded yet."))?;
    let definition = store.experiment(experiment).map_err(server_error)?;
    let versions: BTreeSet<_> = definition
        .benchmarks
        .iter()
        .filter(|benchmark| benchmark.kind == mcm::KIND)
        .map(|benchmark| benchmark.version)
        .collect();
    if versions.len() > 1 {
        return Err(ServerFnError::new(
            "Experiment contains multiple MCM procedure versions.",
        ));
    }
    let version = versions.first().copied().unwrap_or(mcm::VERSION);
    let query = HistoryQuery {
        experiment,
        benchmark_kind: mcm::KIND.into(),
        benchmark_version: version,
        benchmark_parameters: None,
    };
    let history = store.history::<McmOutput>(&query).map_err(server_error)?;
    let snapshot = McmView
        .snapshot(
            experiment,
            &history,
            &[McmMetric::RuntimeRatio, McmMetric::AreaSavings],
        )
        .map_err(server_error)?;
    serde_json::to_string(&snapshot).map_err(|error| ServerFnError::new(error.to_string()))
}

#[cfg(feature = "server")]
fn server_error(error: impl std::fmt::Display) -> ServerFnError {
    ServerFnError::new(error.to_string())
}

#[allow(non_snake_case)]
fn App() -> Element {
    let snapshot = use_resource(load_latest_snapshot);

    match snapshot.read().as_ref() {
        Some(Ok(data)) => rsx! { Results { snapshot_json: data.clone() } },
        Some(Err(error)) => rsx! {
            main { class: "error-page",
                h1 { "Unable to load benchmark results" }
                p { "{error}" }
                form { method: "get", action: "/",
                    button { "Retry" }
                }
            }
        },
        None => rsx! {
            main { class: "loading-page",
                p { "Loading results from SQLite…" }
            }
        },
    }
}

#[component]
fn Results(snapshot_json: String) -> Element {
    use_effect(move || {
        let eval = document::eval(GRAPH_SCRIPT);
        if let Err(error) = eval.send(snapshot_json.clone()) {
            eprintln!("Unable to initialize graph: {error}");
        }
    });

    rsx! {
        style { dangerous_inner_html: RESULTS_CSS }
        header { class: "masthead",
            span { class: "brand", "Yosys", span { class: "brand-slash", " / " }, "benchmarks" }
            form { method: "get", action: "/", class: "refresh-form",
                button { "Refresh results" }
            }
        }
        main {
            header { class: "page-heading",
                div {
                    p { class: "eyebrow", "SYNTHESIS OVER TIME" }
                    h1 { "MCM benchmark history" }
                    p { id: "snapshot-meta", class: "muted" }
                }
                span { id: "experiment", class: "badge" }
            }
            section { class: "controls", aria_label: "Graph controls",
                label { class: "field", "Metric", select { id: "metric" } }
                label { class: "field", "Input projection",
                    select { id: "projection",
                        option { value: "all", "All inputs: aggregate" }
                        option { value: "single", "One coefficient set" }
                        option { value: "compare", "Compare coefficient sets" }
                    }
                }
                label { class: "field workload-field", id: "single-control", hidden: true,
                    "Coefficient set", select { id: "workload" }
                }
                fieldset { id: "compare-control", hidden: true,
                    legend { "Coefficient sets ", span { class: "muted", "Individual lines; no subset averaging" } }
                    div { id: "compare-inputs" }
                }
            }
            section { class: "chart-card", aria_label: "Commit history",
                header { class: "chart-heading",
                    div { h2 { id: "chart-title" }, p { id: "metric-description", class: "muted" } }
                    div { class: "toolbar", aria_label: "Mouse navigation",
                        button { id: "pan", aria_pressed: "true", "Pan" }
                        button { id: "zoom", aria_pressed: "false", "Box zoom" }
                        button { id: "reset", "Reset" }
                        button { id: "export", "Export SVG" }
                    }
                }
                div { class: "plot-wrap",
                    div { id: "plot", role: "img", aria_label: "Interactive benchmark measurements by commit" }
                    div { id: "empty", hidden: true }
                }
                div { class: "chart-footer",
                    span { "Scroll to zoom · Drag to pan · Double-click to reset · Click a point for details" }
                    span { id: "reference-key" }
                }
            }
            p { id: "coverage-note", role: "status" }
            section { class: "details-card", aria_labelledby: "details-title",
                header { class: "details-heading",
                    div { p { class: "eyebrow", "POINT DETAILS" }, h2 { id: "details-title", "Select a commit" } }
                    label { class: "commit-field", "Commit", select { id: "commit", option { value: "", "Choose a commit" } } }
                }
                div { id: "point-details",
                    p { class: "muted", "Click a measurement or a gray missing-data marker to inspect its value and input coverage." }
                }
            }
            details { id: "input-details",
                summary { "Frozen input parameters" }
                div { id: "input-parameters" }
            }
            footer { class: "page-footer",
                "Frozen commit order · Latest finished measurement per input · No pooling of retries"
                br {}
                "Refresh this page to load the latest saved results."
            }
            p { id: "error", role: "alert", hidden: true }
        }
    }
}

const GRAPH_SCRIPT: &str = concat!(
    "/*\n",
    include_str!("../../ui/vendor/plotly.LICENSE"),
    "\n*/\n",
    include_str!("../../ui/vendor/plotly-basic.min.js"),
    "\n",
    include_str!("../../ui/results.js")
);

const RESULTS_CSS: &str = concat!(
    include_str!("../../ui/results.css"),
    "\n.loading-page, .error-page { max-width: 900px; margin: 15vh auto; padding: 24px; }\n",
    ".refresh-form { display: inline; }\n"
);
