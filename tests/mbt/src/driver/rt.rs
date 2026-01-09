use std::future::Future;

use anyhow::Result;
use tempfile::TempDir;
use tokio::runtime::Runtime as TokioRt;

pub struct Runtime {
    pub tokio: TokioRt,
    pub temp_dir: TempDir,
}

impl Runtime {
    pub fn new() -> Result<Self> {
        Ok(Self {
            tokio: TokioRt::new()?,
            temp_dir: TempDir::with_prefix("mbt-emerald-app")?,
        })
    }

    pub fn block_on<F>(&self, f: F) -> F::Output
    where
        F: Future,
    {
        self.tokio.block_on(f)
    }
}
