# SQLite storage

Open a store with `SqliteStore::open(path)?`. It creates missing parent
directories and initializes schema version 1. `:memory:` creates a temporary
database. The crate uses rusqlite with bundled SQLite; no system SQLite package
is required. Unsupported schema versions fail without migrating or replacing data.

The schema is in [schema.sql](../src/store/sqlite/schema.sql). Foreign keys are
enabled on each connection. Registration, attempt start, build failure recording,
and commit completion use transactions. Finalization updates one pending attempt
atomically. No write transaction remains open during benchmark execution.

| Table | Stored data |
| --- | --- |
| `experiments` | Serialized configuration and storage-assigned ID |
| `commits` | Immutable commit metadata, shared across experiments |
| `experiment_commits` | Frozen revision order and completion flags |
| `benchmarks` | Frozen workload position, kind, version, parameters |
| `runs` | Attempt lifecycle, inspection link, artifact path, typed outcome |
| `build_failures` | Commit, time, and error reason |
| `reference_results` | Immutable cached observations, key, time, and ID |

Configuration, commit metadata, timestamps, parameters, and domain payloads use
Serde JSON. Relational keys identify experiments, workload positions, commits,
and attempts. Matching workload definitions may occupy distinct positions;
completion requires each position to have its own measurement.

## Lifecycle

Registering always creates a new experiment and freezes its workload/revision
order. Invalid definitions roll back completely. Starting an attempt validates
its commit metadata, workload position, kind, version, and parameters against
those definitions. It commits the start before returning its ID.

An attempt may be finalized once. Serialization failure leaves it unfinished;
repeated finalization does not overwrite a result. `unfinished_runs(experiment)`
returns pending starts after a crash or reopen, including inspection attempts.
`experiment(id)` reloads the original frozen definition.

Commit completion requires the latest measurement attempt at every workload
position to be finished, and every preceding selected commit to be complete.
Failures and unsupported outcomes count as terminal. A finished retry may
supersede an older interrupted attempt, which remains in the database.

Starting another measurement reopens that commit. `last_complete` stops before
the first incomplete commit, even if later commits retain completion flags.
Completing the retry restores the prefix without discarding later results.
A build failure also reopens its commit and preserves the failure reason.
Automatic resume and successful-build acquisition remain outside this backend.

## History selection

History validates the experiment and declared benchmark kind/version. The caller
chooses the Rust output type corresponding to that declaration. Incompatible
payloads return deserialization errors. Exact parameter filtering compares JSON
values; a parameter filter matching no workload returns an empty history.

Results are ordered by frozen revision position, then workload position. For
each pair, history returns the latest **finished measurement**, including a
failure that supersedes an earlier success. A newer unfinished retry does not
hide the last finished result; use `unfinished_runs` and completion status to
inspect pending work. All attempts remain stored; retries are not extra samples
in the returned history.

Inspections must link to a finished measurement of the same experiment, commit,
and workload position. They do not reopen completion and are excluded from
measurement history. Artifact paths are metadata only: the benchmark/context
owns writing and retaining the files.

## Reference cache

Insertion always appends a new reference ID. Lookup by key returns the latest
entry; refresh never rewrites values used by older comparisons. Comparisons can
retain a reference ID and its value in their serialized output.

The cache stores the supplied key and payload. The benchmark remains responsible
for including tool identity, workload, settings, parser/sampling versions, and a
compatible measurement environment in its cache identity or payload. The backend
does not infer these facts or decide when timing measurements should be refreshed.
