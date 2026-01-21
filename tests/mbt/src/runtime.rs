use core::future::Future;

use anyhow::Result;
use tempfile::TempDir;
use tokio::runtime::Runtime as TokioRt;

/// The test execution runtime.
///
/// A runtime is composed of a Tokio runtime for running async tasks, and a
/// temporary directory that is cleaned after the test is done.
pub struct Runtime {
    pub tokio: TokioRt,
    pub temp_dir: TempDir,
}

impl Runtime {
    pub fn new(temp_dir: TempDir) -> Result<Self> {
        Ok(Self {
            tokio: TokioRt::new()?,
            temp_dir,
        })
    }

    #[inline]
    pub fn block_on<F>(&self, f: F) -> F::Output
    where
        F: Future,
    {
        self.tokio.block_on(f)
    }
}
