// Source for table generation: see docs/schema.md after edits.
// SQLite integers remain 64-bit. Relationships and validation live in Rust.
diesel::table! {
    experiments (id) {
        id -> BigInt,
        configuration -> Text,
    }
}

diesel::table! {
    commits (id) {
        id -> Text,
        metadata -> Text,
    }
}

diesel::table! {
    experiment_commits (experiment_id, commit_id) {
        experiment_id -> BigInt,
        commit_id -> Text,
        position -> BigInt,
        completed -> BigInt,
    }
}

diesel::table! {
    benchmarks (experiment_id, position) {
        experiment_id -> BigInt,
        position -> BigInt,
        kind -> Text,
        version -> BigInt,
        parameters -> Text,
    }
}

diesel::table! {
    runs (id) {
        id -> BigInt,
        experiment_id -> BigInt,
        commit_id -> Text,
        benchmark_index -> BigInt,
        started_at -> Text,
        finished_at -> Nullable<Text>,
        inspection_source -> Nullable<BigInt>,
        artifacts -> Nullable<Text>,
        outcome -> Nullable<Text>,
    }
}

diesel::table! {
    build_failures (id) {
        id -> BigInt,
        experiment_id -> BigInt,
        commit_id -> Text,
        recorded_at -> Text,
        reason -> Text,
    }
}

diesel::table! {
    reference_results (id) {
        id -> BigInt,
        cache_key -> Text,
        measured_at -> Text,
        payload -> Text,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    experiments,
    commits,
    experiment_commits,
    benchmarks,
    runs,
    build_failures,
    reference_results,
);
