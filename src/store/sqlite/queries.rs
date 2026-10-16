use diesel::{dsl::max, prelude::*, sqlite::Sqlite};

use super::{
    rows::RunRow,
    schema::{experiment_commits, runs},
};

#[derive(Clone, Copy)]
pub(super) enum RunSelection {
    LatestFinished,
    AllFinished,
    Unfinished,
    LatestStarted,
}

/// Select IDs in SQLite so old payloads are not loaded just to discard retries.
pub(super) fn select_runs(
    connection: &mut SqliteConnection,
    experiment: i64,
    positions: Option<&[i64]>,
    commit: Option<&str>,
    selection: RunSelection,
) -> QueryResult<Vec<RunRow>> {
    let mut query = runs::table
        .inner_join(
            experiment_commits::table.on(experiment_commits::experiment_id
                .eq(runs::experiment_id)
                .and(experiment_commits::commit_id.eq(runs::commit_id))),
        )
        .filter(runs::experiment_id.eq(experiment))
        .order((
            experiment_commits::position.asc(),
            runs::benchmark_index.asc(),
            runs::id.asc(),
        ))
        .select(RunRow::as_select())
        .into_boxed::<Sqlite>();
    if let Some(positions) = positions {
        query = query.filter(runs::benchmark_index.eq_any(positions));
    }
    if let Some(commit) = commit {
        query = query.filter(runs::commit_id.eq(commit));
    }
    match selection {
        RunSelection::Unfinished => query = query.filter(runs::finished_at.is_null()),
        RunSelection::AllFinished => {
            query = query
                .filter(runs::inspection_source.is_null())
                .filter(runs::finished_at.is_not_null());
        }
        RunSelection::LatestFinished | RunSelection::LatestStarted => {
            let finished_only = matches!(selection, RunSelection::LatestFinished);
            let latest = diesel::alias!(runs as latest_measurements);
            let latest_ids = latest
                .filter(latest.field(runs::experiment_id).eq(experiment))
                .filter(latest.field(runs::inspection_source).is_null())
                .filter(
                    latest
                        .field(runs::finished_at)
                        .is_not_null()
                        .or::<_, diesel::sql_types::Bool>(!finished_only),
                )
                .group_by((
                    latest.field(runs::commit_id),
                    latest.field(runs::benchmark_index),
                ))
                .select(max(latest.field(runs::id)));
            query = query.filter(runs::id.nullable().eq_any(latest_ids));
        }
    }
    query.load(connection)
}
