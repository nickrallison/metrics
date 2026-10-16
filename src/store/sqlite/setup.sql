CREATE INDEX runs_workload ON runs(experiment_id, commit_id, benchmark_index, id);
CREATE INDEX reference_results_key ON reference_results(cache_key, id);

PRAGMA user_version = 1;
