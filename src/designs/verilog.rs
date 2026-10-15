use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct VerilogDesign {
    top: String,
    files: Vec<PathBuf>,
}
