use std::future::Future;

use anyhow::{bail, Result};

use crate::driver::EmeraldDriver;
use crate::history::History;
use crate::state::Node;
use crate::sut::Sut;

impl EmeraldDriver {
    pub fn perform<'a, F, Fut>(&'a mut self, node: Node, action: F) -> Result<()>
    where
        F: FnOnce(&'a mut Sut, &'a mut History) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let Some(sut) = self.sut.get_mut(&node) else {
            bail!("Unknown node: {}", node)
        };
        let Some(rt) = &mut self.runtime else {
            bail!("Runtime is uninitialized")
        };
        rt.block_on(action(sut, &mut self.history))
    }
}
