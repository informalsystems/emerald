use core::future::Future;

use tempfile::TempDir;
use tokio::runtime::Runtime as TokioRt;

pub struct Runtime {
    pub tokio: TokioRt,
    pub temp_dir: TempDir,
}

impl Runtime {
    pub fn block_on<F>(&self, f: F) -> F::Output
    where
        F: Future,
    {
        self.tokio.block_on(f)
    }
}
