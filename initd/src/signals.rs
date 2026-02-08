// ABOUTME: Signal handling for PID 1.
// ABOUTME: Registers handlers for SIGCHLD, SIGTERM, SIGINT, and SIGUSR1.

use signal_hook::consts::{SIGCHLD, SIGINT, SIGTERM, SIGUSR1};
use signal_hook::flag;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct SignalState {
    pub shutdown_requested: Arc<AtomicBool>,
    pub child_exited: Arc<AtomicBool>,
    pub reload_requested: Arc<AtomicBool>,
}

impl SignalState {
    pub fn register() -> std::io::Result<Self> {
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let child_exited = Arc::new(AtomicBool::new(false));
        let reload_requested = Arc::new(AtomicBool::new(false));

        flag::register(SIGTERM, Arc::clone(&shutdown_requested))?;
        flag::register(SIGINT, Arc::clone(&shutdown_requested))?;
        flag::register(SIGCHLD, Arc::clone(&child_exited))?;
        flag::register(SIGUSR1, Arc::clone(&reload_requested))?;

        Ok(Self {
            shutdown_requested,
            child_exited,
            reload_requested,
        })
    }

    pub fn is_shutdown_requested(&self) -> bool {
        self.shutdown_requested.load(Ordering::Relaxed)
    }

    pub fn take_child_exited(&self) -> bool {
        self.child_exited.swap(false, Ordering::Relaxed)
    }

    pub fn take_reload_requested(&self) -> bool {
        self.reload_requested.swap(false, Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_is_clear() {
        let state = SignalState::register().unwrap();
        assert!(!state.is_shutdown_requested());
        assert!(!state.take_child_exited());
        assert!(!state.take_reload_requested());
    }

    #[test]
    fn take_child_exited_resets_flag() {
        let state = SignalState::register().unwrap();
        state.child_exited.store(true, Ordering::Relaxed);
        assert!(state.take_child_exited());
        assert!(!state.take_child_exited()); // should be reset
    }
}
