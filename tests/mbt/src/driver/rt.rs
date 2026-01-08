use std::future::Future;

use anyhow::Result;
use tempfile::TempDir;
use tokio::runtime::Runtime as TokioRt;

use crate::reth::{self, RethHandle};

pub struct Runtime {
    pub tempdir: TempDir,
    pub tokio: TokioRt,
    // TODO: have a reth instace per emerald node.
    _reth: RethHandle,
}

impl Runtime {
    pub fn new() -> Result<Self> {
        Ok(Self {
            tokio: TokioRt::new()?,
            tempdir: TempDir::with_prefix("mbt-emerald-app")?,
            _reth: reth::start()?,
        })
    }

    pub fn block_on<F>(&self, f: F) -> F::Output
    where
        F: Future,
    {
        self.tokio.block_on(f)
    }
}
