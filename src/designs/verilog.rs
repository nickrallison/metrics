use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct VerilogDesign {
    top: String,
    files: Vec<PathBuf>,
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
