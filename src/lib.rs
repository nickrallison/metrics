//! Typed benchmark histories. Execution and persistence are scaffolds for now.

pub mod benchmark;
pub mod benchmarks;
pub mod build;
pub mod design;
pub mod error;
pub mod experiment;
pub mod history;
pub mod store;
pub mod suite;
pub mod view;

pub use error::{Error, Result};
