use std::path::Path;

use crate::VerilogDesign;

#[derive(Debug, Clone)]
pub struct Commands {
    commands: Vec<String>,
}

impl Commands {
    pub fn new(commands: Vec<String>) -> Self {
        Self { commands }
    }

    fn default_read(verilog_design: &VerilogDesign) -> Self {
        let mut commands = vec![];

        for file in verilog_design.files() {
            commands.push(format!("read -sv \"{}\"", file.display()));
        }

        Self::new(commands)
    }

    fn default_synth(top: &str, cell_lib: &Path) -> Self {
        let mut commands = vec![];

        commands.push(format!("synth -top {} -noabc", top));
        commands.push(format!("dfflibmap -liberty \"{}\"", &cell_lib.display()));
        commands.push(format!("abc -liberty \"{}\"", &cell_lib.display()));
        commands.push("clean".into());

        Self::new(commands)
    }

    fn default_output(stat_log_out: &Path, cell_lib: &Path) -> Self {
        let mut commands = vec![];

        commands.push(format!(
            "tee -o \"{}\" stat -liberty \"{}\"",
            &stat_log_out.display(),
            &cell_lib.display()
        ));

        Self::new(commands)
    }
}
