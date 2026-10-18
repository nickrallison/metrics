//! Typed benchmark histories. Execution and persistence are scaffolds for now.
//!
//! A suite freezes a collection of benchmarks. Each benchmark owns one complete
//! comparison, including its tools, parsing, and repetition strategy. An experiment
//! runs that collection sequentially at each selected commit on one branch.
//!
//! The module docs describe the intended behavior of unfinished operations.
//! `docs/scaffold.md` contains the implementation order and open decisions;
//! `examples/reference/` preserves the original MCM experiment and porting notes.

pub mod analysis;
pub mod benchmark;
pub mod benchmarks;
pub mod build;
pub mod design;
pub mod error;
pub mod experiment;
pub mod history;
pub mod output;
pub mod store;
pub mod suite;
pub mod view;

pub use error::{Error, Result};
