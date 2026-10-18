use dioxus::prelude::*;

pub mod analysis;
pub mod components;
pub mod presentation;
#[cfg(feature = "server")]
mod providers;

use components::AnalysisWorkspace;
use presentation::{AnalysisRequest, Catalog};

fn main() {
    #[cfg(feature = "server")]
    dioxus::serve(|| async move { Ok(dioxus::server::router(App)) });

    #[cfg(not(feature = "server"))]
    dioxus::launch(App);
}

#[server]
async fn load_catalog() -> Result<Catalog<String>, ServerFnError> {
    use benchmarks::store::SqliteStore;

    let database = providers::database_path();
    if !database.is_file() {
        return Err(ServerFnError::new(format!(
            "Database {} does not exist. Set BENCHMARKS_DATABASE to the results database.",
            database.display()
        )));
    }
    let store = SqliteStore::open(database).map_err(server_error)?;
    let latest = store
        .latest_experiment()
        .map_err(server_error)?
        .ok_or_else(|| ServerFnError::new("No experiments are recorded yet."))?;
    let definition = store.experiment(latest).map_err(server_error)?;
    let registry = providers::registry();
    let benchmark_types: Vec<_> = definition
        .benchmarks
        .iter()
        .map(|benchmark| (benchmark.kind.clone(), benchmark.version))
        .collect();

    Ok(Catalog {
        experiments: vec![presentation::ExperimentOption {
            id: latest.to_string(),
            label: format!("Experiment {latest}"),
        }],
        analyses: registry.descriptors_for(&benchmark_types),
    })
}

#[server]
async fn run_analysis(
    request: AnalysisRequest<String>,
) -> Result<presentation::AnalysisResult, ServerFnError> {
    providers::registry()
        .run(&request)
        .map_err(ServerFnError::new)
}

#[cfg(feature = "server")]
fn server_error(error: impl std::fmt::Display) -> ServerFnError {
    ServerFnError::new(error.to_string())
}

#[allow(non_snake_case)]
fn App() -> Element {
    let catalog = use_server_future(load_catalog)?;
    match catalog.read().as_ref() {
        Some(Ok(catalog)) => rsx! { Workspace { catalog: catalog.clone() } },
        Some(Err(error)) => rsx! {
            main { class: "state-page",
                h1 { "Unable to load analyses" }
                p { "{error}" }
            }
        },
        None => rsx! {
            main { class: "state-page",
                p { class: "muted", "Loading available analyses…" }
            }
        },
    }
}

#[component]
fn Workspace(catalog: Catalog<String>) -> Element {
    let mut experiment = use_signal(|| catalog.experiments.first().map(|item| item.id.clone()));
    let mut analysis = use_signal(|| {
        catalog
            .analyses
            .first()
            .map(|item| item.id.clone())
            .unwrap_or_default()
    });
    let mut parameters = use_signal(|| default_parameters(catalog.analyses.first()));
    let analysis_catalog = catalog.clone();
    let result = use_resource(move || {
        let experiment = experiment();
        let analysis = analysis();
        let parameters = parameters();
        async move {
            let Some(experiment) = experiment.filter(|_| !analysis.is_empty()) else {
                return Ok(None);
            };
            run_analysis(AnalysisRequest {
                experiment,
                analysis,
                parameters,
            })
            .await
            .map(Some)
        }
    });

    let (data, error, loading) = match result.read().as_ref() {
        Some(Ok(data)) => (data.clone(), None, false),
        Some(Err(error)) => (None, Some(error.to_string()), false),
        None => (None, None, true),
    };

    rsx! {
        AnalysisWorkspace {
            catalog,
            analysis_id: analysis(),
            parameters: parameters(),
            result: data,
            error,
            loading,
            on_experiment_change: move |id: String| experiment.set(Some(id)),
            on_analysis_change: move |id: String| {
                analysis.set(id.clone());
                let descriptor = analysis_catalog.analyses.iter().find(|item| item.id == id);
                parameters.set(default_parameters(descriptor));
            },
            on_parameter_change: move |(id, value): (String, String)| {
                let mut current = parameters();
                current.insert(id, serde_json::Value::String(value));
                parameters.set(current);
            },
        }
    }
}

fn default_parameters(
    analysis: Option<&presentation::AnalysisDescriptor>,
) -> serde_json::Map<String, serde_json::Value> {
    analysis
        .into_iter()
        .flat_map(|analysis| &analysis.parameters)
        .filter_map(|parameter| {
            parameter.default.as_ref().map(|value| {
                (
                    parameter.id.clone(),
                    serde_json::Value::String(value.clone()),
                )
            })
        })
        .collect()
}
