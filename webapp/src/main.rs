use dioxus::prelude::*;

pub mod components;
#[cfg(feature = "server")]
mod providers;

pub use benchmark_analysis_types as presentation;
use benchmark_analysis_types::{AnalysisRequest, Catalog};
#[cfg(feature = "server")]
use benchmarks::{analysis::AnalysisRegistry, store::SqliteStore};
use components::AnalysisWorkspace;

fn main() {
    #[cfg(feature = "server")]
    dioxus::serve(|| async move { Ok(dioxus::server::router(App)) });

    #[cfg(not(feature = "server"))]
    dioxus::launch(App);
}

#[server]
async fn load_catalog() -> Result<Catalog<String>, ServerFnError> {
    let registry = registry();
    let experiments = providers::experiments().map_err(ServerFnError::new)?;
    Ok(Catalog {
        experiments: experiments
            .into_iter()
            .map(|entry| {
                let store = SqliteStore::open(&entry.database).map_err(server_error)?;
                let definition = store.experiment(entry.experiment).map_err(server_error)?;
                let benchmark_types = benchmark_types(&definition);
                Ok(presentation::ExperimentOption {
                    id: entry.id.to_string(),
                    label: format!("{} · {}", entry.label, entry.experiment),
                    analyses: registry.descriptors_for(&benchmark_types),
                })
            })
            .collect::<Result<Vec<_>, ServerFnError>>()?,
    })
}

#[server]
async fn run_analysis(
    request: AnalysisRequest<String>,
) -> Result<presentation::AnalysisResult, ServerFnError> {
    let catalog_id = request
        .experiment
        .parse::<i64>()
        .map_err(|_| ServerFnError::new("Invalid experiment selection"))?;
    let entry = providers::experiment(catalog_id)
        .map_err(ServerFnError::new)?
        .ok_or_else(|| ServerFnError::new("The selected experiment is no longer available"))?;
    let store = SqliteStore::open(&entry.database).map_err(server_error)?;
    let definition = store.experiment(entry.experiment).map_err(server_error)?;
    registry()
        .run(
            &request.analysis,
            &store,
            entry.experiment,
            &benchmark_types(&definition),
            &request.parameters,
        )
        .map_err(ServerFnError::new)
}

#[cfg(feature = "server")]
fn registry() -> AnalysisRegistry {
    let mut registry = AnalysisRegistry::default();
    benchmarks::benchmarks::register_analyses(&mut registry);
    registry
}

#[cfg(feature = "server")]
fn benchmark_types(definition: &benchmarks::store::ExperimentDefinition) -> Vec<(String, u32)> {
    definition
        .benchmarks
        .iter()
        .map(|benchmark| (benchmark.kind.clone(), benchmark.version))
        .collect()
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
            .experiments
            .first()
            .and_then(|experiment| experiment.analyses.first())
            .map(|item| item.id.clone())
            .unwrap_or_default()
    });
    let mut parameters = use_signal(|| {
        catalog
            .experiments
            .first()
            .and_then(|experiment| experiment.analyses.first())
            .map(|analysis| default_parameters(Some(analysis)))
            .unwrap_or_default()
    });
    let experiment_catalog = catalog.clone();
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
            selected_experiment: experiment().unwrap_or_default(),
            analysis_id: analysis(),
            parameters: parameters(),
            result: data,
            error,
            loading,
            on_experiment_change: move |id: String| {
                experiment.set(Some(id.clone()));
                let descriptor = experiment_catalog
                    .experiments
                    .iter()
                    .find(|item| item.id == id)
                    .and_then(|item| item.analyses.first());
                analysis.set(descriptor.map(|item| item.id.clone()).unwrap_or_default());
                parameters.set(default_parameters(descriptor));
            },
            on_analysis_change: move |id: String| {
                analysis.set(id.clone());
                let descriptor = analysis_catalog
                    .experiments
                    .iter()
                    .find(|item| Some(&item.id) == experiment().as_ref())
                    .and_then(|item| item.analyses.iter().find(|item| item.id == id));
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
