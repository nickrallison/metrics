-- Generated from schema.rs with Diesel CLI. See docs/schema.md to regenerate.
-- Your SQL goes here
CREATE TABLE `runs`(
	`id` INTEGER NOT NULL PRIMARY KEY,
	`experiment_id` BIGINT NOT NULL,
	`commit_id` TEXT NOT NULL,
	`benchmark_index` BIGINT NOT NULL,
	`started_at` TEXT NOT NULL,
	`finished_at` TEXT,
	`inspection_source` BIGINT,
	`artifacts` TEXT,
	`outcome` TEXT
);

CREATE TABLE `commits`(
	`id` TEXT NOT NULL PRIMARY KEY,
	`metadata` TEXT NOT NULL
);

CREATE TABLE `experiment_commits`(
	`experiment_id` BIGINT NOT NULL,
	`commit_id` TEXT NOT NULL,
	`position` BIGINT NOT NULL,
	`completed` BIGINT NOT NULL,
	PRIMARY KEY(`experiment_id`, `commit_id`)
);

CREATE TABLE `experiments`(
	`id` INTEGER NOT NULL PRIMARY KEY,
	`configuration` TEXT NOT NULL
);

CREATE TABLE `benchmarks`(
	`experiment_id` BIGINT NOT NULL,
	`position` BIGINT NOT NULL,
	`kind` TEXT NOT NULL,
	`version` BIGINT NOT NULL,
	`parameters` TEXT NOT NULL,
	PRIMARY KEY(`experiment_id`, `position`)
);

CREATE TABLE `build_failures`(
	`id` INTEGER NOT NULL PRIMARY KEY,
	`experiment_id` BIGINT NOT NULL,
	`commit_id` TEXT NOT NULL,
	`recorded_at` TEXT NOT NULL,
	`reason` TEXT NOT NULL
);

CREATE TABLE `reference_results`(
	`id` INTEGER NOT NULL PRIMARY KEY,
	`cache_key` TEXT NOT NULL,
	`measured_at` TEXT NOT NULL,
	`payload` TEXT NOT NULL
);
