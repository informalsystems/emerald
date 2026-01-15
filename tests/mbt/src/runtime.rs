use core::future::Future;

use anyhow::Result;
use tempfile::TempDir;
use tokio::runtime::Runtime as TokioRt;

pub struct Runtime {
    pub tokio: TokioRt,
    pub temp_dir: TempDir,
}

impl Runtime {
    pub fn new(temp_dir: TempDir) -> Result<Self> {
        Ok(Self {
            tokio: tokio::runtime::Runtime::new()?,
            temp_dir,
        })
    }

    pub fn block_on<F>(&self, f: F) -> F::Output
    where
        F: Future,
    {
        self.tokio.block_on(f)
    }
}
