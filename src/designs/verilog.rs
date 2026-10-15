use std::path::PathBuf;

use super::Design;

#[derive(Debug, Clone)]
pub struct VerilogDesign {
    top: String,
    files: Vec<PathBuf>,
}

impl Design for VerilogDesign {
    fn into_verilog(&self) -> VerilogDesign {
        self.clone()
    }
}

impl VerilogDesign {
    pub fn new(top: String, files: Vec<PathBuf>) -> Self {
        Self { top, files }
    }

    pub fn top(&self) -> &str {
        &self.top
    }

    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }
}
