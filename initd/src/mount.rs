// ABOUTME: Early filesystem mounting for the init system.
// ABOUTME: Mounts /proc, /sys, /dev, /tmp, /run before services start.

use rustix::mount::{mount, MountFlags};
use std::ffi::{CStr, CString};
use tracing::{error, info};

struct MountPoint {
    source: &'static str,
    target: &'static str,
    fstype: &'static str,
    flags: MountFlags,
}

const EARLY_MOUNTS: &[MountPoint] = &[
    MountPoint {
        source: "proc",
        target: "/proc",
        fstype: "proc",
        flags: MountFlags::NOSUID,
    },
    MountPoint {
        source: "sysfs",
        target: "/sys",
        fstype: "sysfs",
        flags: MountFlags::NOSUID,
    },
    MountPoint {
        source: "devtmpfs",
        target: "/dev",
        fstype: "devtmpfs",
        flags: MountFlags::NOSUID,
    },
    MountPoint {
        source: "tmpfs",
        target: "/tmp",
        fstype: "tmpfs",
        flags: MountFlags::NOSUID,
    },
    MountPoint {
        source: "tmpfs",
        target: "/run",
        fstype: "tmpfs",
        flags: MountFlags::NOSUID,
    },
];

pub fn mount_early_filesystems() {
    for mp in EARLY_MOUNTS {
        let _ = std::fs::create_dir_all(mp.target);

        let source = CString::new(mp.source).unwrap();
        let fstype = CString::new(mp.fstype).unwrap();

        match mount(&source, mp.target, &fstype, mp.flags, None::<&CStr>) {
            Ok(()) => info!(target = mp.target, fstype = mp.fstype, "mounted"),
            Err(e) => error!(target = mp.target, fstype = mp.fstype, error = %e, "mount failed"),
        }
    }
}
