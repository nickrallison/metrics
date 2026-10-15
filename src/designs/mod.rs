pub mod verilog;
use verilog::VerilogDesign;

pub trait Design {
    fn into_verilog(&self) -> VerilogDesign {}
}
