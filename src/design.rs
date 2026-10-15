//! Prepared Verilog inputs; other benchmarks can use unrelated representations.
//!
//! File-backed workloads remain fixed within an experiment. Before execution,
//! record content digests for sources, included files, libraries, and generator
//! dependencies alongside the serialized definitions. Source order, language,
//! top, defines, and generator parameters also affect workload identity.
//! Verify these inputs on resume/inspection; a matching path or label is not a
//! content check. Digest acquisition and storage are not implemented yet.
//!
//! Suite preparation may adapt an existing benchmark manifest. Keep preparation
//! outside measured subprocesses unless it is explicitly the subject under test.

use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceLanguage {
    Verilog,
    SystemVerilog,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: PathBuf,
    pub language: SourceLanguage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerilogDesign {
    pub top: String,
    pub sources: Vec<SourceFile>,
    pub include_dirs: Vec<PathBuf>,
    pub defines: BTreeMap<String, Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellLibrary {
    pub name: String,
    pub liberty: PathBuf,
}
