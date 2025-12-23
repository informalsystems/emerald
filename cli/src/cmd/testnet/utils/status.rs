use tracing::info;

use crate::cmd::testnet::ProcessHandle;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NodeStatus {
    Running,
    Stopped,
    Uninitialised,
}

pub fn is_node_running(handle: &ProcessHandle) -> NodeStatus {
    if handle.is_running() {
        info!("Running (PID: {})", handle.pid);
        NodeStatus::Running
    } else {
        NodeStatus::Stopped
    }
}
