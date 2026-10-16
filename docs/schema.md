# SQLite schema

Edit `src/store/sqlite/schema.rs` to define tables, columns, and primary keys.
With Diesel CLI 2.3 installed, generate tables from the project root:

```sh
schema_preview=$(mktemp -d)
mkdir "$schema_preview/migrations"
touch "$schema_preview/empty.sqlite"
diesel migration generate initial \
    --config-file diesel.toml \
    --migration-dir "$schema_preview/migrations" \
    --database-url "$schema_preview/empty.sqlite" \
    --version 1 --no-down --diff-schema \
    --sqlite-integer-primary-key-is-bigint
cp $schema_preview/migrations/1_initial/up.sql src/store/sqlite/schema.sql
```

Review `$schema_preview/migrations/1_initial/up.sql`, then copy it to
`src/store/sqlite/schema.sql`. Remove the temporary directory when finished.
Commit the generated SQL so normal builds and execution do not require Diesel CLI.
Generation does not access experiment databases or run migrations. Keep
`schema.rs` as the source; do not replace it with `diesel print-schema` output.

`src/store/sqlite/setup.sql` contains the two lookup indexes and sets
`PRAGMA user_version = 1`. Opening a version-zero database executes both SQL files
in one transaction. Version one skips initialization; other versions are rejected.
The marker does not verify table definitions or migrate existing databases.

Relationships and state changes are checked by `SqliteStore` in Rust. Invalid
experiments, commits, workloads, inspection sources, duplicate commit positions,
and repeated finalization return typed `Error` variants. Unsigned Rust positions
and versions enforce nonnegative inputs; completion is set explicitly from a bool.
Primary keys and `NOT NULL` remain in SQL. Direct database edits bypass the Rust
checks.

Existing databases are left untouched, including any constraints they already
have. Recreating a disposable development database uses the generated definitions;
changing `schema.rs` alone does not change an existing database.
