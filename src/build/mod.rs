//! Obtain a tool installation for one commit.
//!
//! Initially there is one fixed build procedure. Cache by commit within a
//! repository-specific directory. Changing the procedure requires invalidating or
//! separating that cache; configurable build profiles are deferred.
//!
//! Build in an isolated checkout and keep the executable and its required data
//! files together. Validate a cached installation before returning it. Record
//! build failures against the commit and leave that commit incomplete.
//! Building must finish before timed benchmark work starts.

mod yosys;

pub use yosys::{Yosys, YosysBuilder};

use crate::{Result, history::Commit};

pub trait Builder {
    type Tool;

    fn build(&mut self, commit: &Commit) -> Result<Self::Tool>;
}
