//! Obtain a tool installation for one commit.

mod yosys;

pub use yosys::{Yosys, YosysBuilder};

use crate::{Result, history::Commit};

pub trait Builder {
    type Tool;

    fn build(&mut self, commit: &Commit) -> Result<Self::Tool>;
}
