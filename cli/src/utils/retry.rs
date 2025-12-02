use core::time::Duration;
use std::thread::sleep;
use std::time::Instant;

use color_eyre::eyre::eyre;
use color_eyre::Result;

pub fn retry_with_timeout<F, T>(
    task_name: &str,
    timeout: Duration,
    interval: Duration,
    mut f: F,
) -> Result<T>
where
    F: FnMut() -> Result<T>,
{
    let start = Instant::now();

    loop {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) => {
                if start.elapsed() >= timeout {
                    return Err(eyre!(
                        "task {task_name} failed after {} seconds. Cause: {e}",
                        timeout.as_secs()
                    ));
                }
                sleep(interval);
            }
        }
    }
}
