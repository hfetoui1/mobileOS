// ABOUTME: MobileOS init system (PID 1).
// ABOUTME: Mounts filesystems, starts services, and supervises the process tree.

mod config;
mod dependency;
mod logging;
mod mount;
mod service;

use rustix::process::{getpid, waitpid, WaitOptions};
use std::process::Command;
use tracing::{error, info, warn};

fn spawn_shell() -> Option<u32> {
    info!("spawning /bin/sh");
    match Command::new("/bin/sh").spawn() {
        Ok(child) => Some(child.id()),
        Err(e) => {
            error!(error = %e, "failed to spawn shell");
            None
        }
    }
}

fn reap_zombies() {
    while let Ok(Some(_)) = waitpid(None, WaitOptions::NOHANG) {}
}

fn main() {
    logging::init();

    let pid = getpid();
    info!(pid = pid.as_raw_nonzero().get(), "MobileOS init starting");

    mount::mount_early_filesystems();

    // TODO: Phase 1 — load service configs and start service manager
    // For now, fall back to spawning a shell directly
    let shell_pid = spawn_shell();

    if shell_pid.is_none() {
        warn!("no shell available, halting");
        loop {
            reap_zombies();
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }

    // PID 1 must never exit — reap children forever
    loop {
        match waitpid(None, WaitOptions::empty()) {
            Ok(Some((child_pid, status))) => {
                info!(
                    pid = child_pid.as_raw_nonzero().get(),
                    status = ?status,
                    "child exited"
                );
            }
            _ => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        reap_zombies();
    }
}
