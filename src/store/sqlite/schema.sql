CREATE TABLE experiments (
    id INTEGER PRIMARY KEY,
    configuration TEXT NOT NULL
);

CREATE TABLE commits (
    id TEXT PRIMARY KEY,
    metadata TEXT NOT NULL
);

CREATE TABLE experiment_commits (
    experiment_id INTEGER NOT NULL REFERENCES experiments(id),
    commit_id TEXT NOT NULL REFERENCES commits(id),
    position INTEGER NOT NULL CHECK(position >= 0),
    completed INTEGER NOT NULL DEFAULT 0 CHECK(completed IN (0, 1)),
    PRIMARY KEY (experiment_id, commit_id),
    UNIQUE (experiment_id, position)
);

CREATE TABLE benchmarks (
    experiment_id INTEGER NOT NULL REFERENCES experiments(id),
    position INTEGER NOT NULL CHECK(position >= 0),
    kind TEXT NOT NULL,
    version INTEGER NOT NULL CHECK(version >= 0),
    parameters TEXT NOT NULL,
    PRIMARY KEY (experiment_id, position)
);

CREATE TABLE runs (
    id INTEGER PRIMARY KEY,
    experiment_id INTEGER NOT NULL,
    commit_id TEXT NOT NULL,
    benchmark_index INTEGER NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    inspection_source INTEGER REFERENCES runs(id),
    artifacts TEXT,
    outcome TEXT,
    FOREIGN KEY (experiment_id, commit_id)
        REFERENCES experiment_commits(experiment_id, commit_id),
    FOREIGN KEY (experiment_id, benchmark_index)
        REFERENCES benchmarks(experiment_id, position),
    CHECK ((finished_at IS NULL) = (outcome IS NULL))
);

CREATE INDEX runs_workload ON runs(experiment_id, commit_id, benchmark_index, id);

CREATE TABLE build_failures (
    id INTEGER PRIMARY KEY,
    experiment_id INTEGER NOT NULL,
    commit_id TEXT NOT NULL,
    recorded_at TEXT NOT NULL,
    reason TEXT NOT NULL,
    FOREIGN KEY (experiment_id, commit_id)
        REFERENCES experiment_commits(experiment_id, commit_id)
);

CREATE TABLE reference_results (
    id INTEGER PRIMARY KEY,
    cache_key TEXT NOT NULL,
    measured_at TEXT NOT NULL,
    payload TEXT NOT NULL
);

CREATE INDEX reference_results_key ON reference_results(cache_key, id);

PRAGMA user_version = 1;
