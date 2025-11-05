//! Multithreaded runtime builder.

use std::io::Result;

use malachitebft_config::RuntimeConfig;
use tokio::runtime::{Builder as RtBuilder, Runtime};

pub fn build_runtime(cfg: RuntimeConfig) -> Result<Runtime> {
    let mut builder = match cfg {
        RuntimeConfig::SingleThreaded => RtBuilder::new_current_thread(),
        RuntimeConfig::MultiThreaded { worker_threads } => {
            let mut builder = RtBuilder::new_multi_thread();
            if worker_threads > 0 {
                builder.worker_threads(worker_threads);
            }
            builder
        }
    };

    builder.enable_all().build()
}
