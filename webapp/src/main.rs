use dioxus::prelude::*;

pub mod components;
mod controls;
#[cfg(feature = "server")]
mod providers;

pub use benchmark_analysis_types as presentation;
use benchmark_analysis_types::{AnalysisPlan, AnalysisRequest, Catalog, PlotOptions};
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
                Ok(presentation::ExperimentOption {
                    id: entry.id.to_string(),
                    label: format!("{} · {}", entry.label, entry.experiment),
                    analyses: registry.descriptors_for(&definition),
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
            &definition,
            &request.plan,
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

#[derive(Debug, Clone, PartialEq)]
struct WorkspaceSelection {
    experiment: String,
    analysis: String,
    plan: Option<AnalysisPlan>,
}

impl WorkspaceSelection {
    fn for_experiment(experiment: Option<&presentation::ExperimentOption<String>>) -> Self {
        let descriptor = experiment.and_then(|e| e.analyses.first());
        Self {
            experiment: experiment.map(|e| e.id.clone()).unwrap_or_default(),
            analysis: descriptor.map(|a| a.id.clone()).unwrap_or_default(),
            plan: descriptor.map(AnalysisPlan::for_descriptor),
        }
    }
}

#[component]
fn Workspace(catalog: Catalog<String>) -> Element {
    let mut selection = use_signal(|| WorkspaceSelection::for_experiment(catalog.experiments.first()));
    let mut plot = use_signal(PlotOptions::default);
    let result = use_resource(move || {
        let snapshot = selection();
        async move {
            let data = if let Some(plan) = snapshot.plan.clone() {
                run_analysis(AnalysisRequest {
                    experiment: snapshot.experiment.clone(), analysis: snapshot.analysis.clone(), plan,
                }).await.map(Some).map_err(|e| e.to_string())
            } else { Ok(None) };
            (snapshot, data)
        }
    });
    // A resource can retain its previous value while refreshing. Only display
    // a response that belongs to the complete current selection and plan.
    let current = selection();
    let (data, error, loading) = match result.read().as_ref() {
        Some((snapshot, Ok(data))) if *snapshot == current => (data.clone(), None, false),
        Some((snapshot, Err(error))) if *snapshot == current => (None, Some(error.clone()), false),
        _ => (None, None, true),
    };
    let experiments = catalog.clone();
    let analyses = catalog.clone();
    rsx! {
        AnalysisWorkspace {
            catalog,
            selected_experiment: current.experiment,
            analysis_id: current.analysis,
            plan: current.plan,
            plot: plot(),
            result: data, error, loading,
            on_experiment_change: move |id: String| {
                selection.set(WorkspaceSelection::for_experiment(experiments.experiments.iter().find(|e| e.id == id)));
            },
            on_analysis_change: move |id: String| {
                let mut next = selection();
                let descriptor = analyses.experiments.iter().find(|e| e.id == next.experiment)
                    .and_then(|e| e.analyses.iter().find(|a| a.id == id));
                next.analysis = id;
                next.plan = descriptor.map(AnalysisPlan::for_descriptor);
                selection.set(next);
            },
            on_plan_change: move |plan| {
                let mut next = selection();
                next.plan = Some(plan);
                selection.set(next);
            },
            on_plot_change: move |options| plot.set(options),
        }
    }
}
