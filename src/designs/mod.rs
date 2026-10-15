pub mod verilog;

#[derive(Debug, Clone)]
pub enum Design {
    Verilog(VerilogDesign),
}
