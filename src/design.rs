//! Prepared Verilog inputs; other benchmarks can use unrelated representations.

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
