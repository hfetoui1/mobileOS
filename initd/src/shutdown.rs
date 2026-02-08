// ABOUTME: Shutdown and reboot handling for the init system.
// ABOUTME: Stops services in reverse order, unmounts filesystems, and halts/reboots.

use rustix::mount::{unmount, UnmountFlags};
use rustix::system::{reboot, RebootCommand};
use tracing::{error, info, warn};

use crate::service::ServiceManager;

const UNMOUNT_ORDER: &[&str] = &["/run", "/tmp", "/dev", "/sys", "/proc"];

pub fn perform_shutdown(manager: &mut ServiceManager) {
    info!("initiating shutdown");

    info!("stopping all services");
    manager.stop_all();

    unmount_filesystems();

    info!("system halted, calling reboot(POWER_OFF)");
    if let Err(e) = reboot(RebootCommand::PowerOff) {
        error!(error = %e, "reboot syscall failed");
    }
}

pub fn perform_reboot(manager: &mut ServiceManager) {
    info!("initiating reboot");

    info!("stopping all services");
    manager.stop_all();

    unmount_filesystems();

    info!("rebooting");
    if let Err(e) = reboot(RebootCommand::Restart) {
        error!(error = %e, "reboot syscall failed");
    }
}

fn unmount_filesystems() {
    for target in UNMOUNT_ORDER {
        match unmount(*target, UnmountFlags::DETACH) {
            Ok(()) => info!(target = target, "unmounted"),
            Err(e) => warn!(target = target, error = %e, "unmount failed"),
        }
    }
}
