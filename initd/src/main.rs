// ABOUTME: MobileOS init system (PID 1).
// ABOUTME: Mounts filesystems, starts services, and supervises the process tree.

use rustix::mount::{mount, MountFlags};
use rustix::process::{getpid, waitpid, WaitOptions};
use std::ffi::{CStr, CString};
use std::process::Command;

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

fn mount_filesystems() {
    for mp in EARLY_MOUNTS {
        // Ensure mount target directory exists
        let _ = std::fs::create_dir_all(mp.target);

        let source = CString::new(mp.source).unwrap();
        let fstype = CString::new(mp.fstype).unwrap();

        match mount(&source, mp.target, &fstype, mp.flags, None::<&CStr>) {
            Ok(()) => eprintln!("[init] mounted {} on {}", mp.fstype, mp.target),
            Err(e) => eprintln!("[init] failed to mount {} on {}: {}", mp.fstype, mp.target, e),
        }
    }
}

fn spawn_shell() -> Option<u32> {
    eprintln!("[init] spawning /bin/sh");
    match Command::new("/bin/sh").spawn() {
        Ok(child) => Some(child.id()),
        Err(e) => {
            eprintln!("[init] failed to spawn shell: {}", e);
            None
        }
    }
}

fn reap_zombies() {
    while let Ok(Some(_)) = waitpid(None, WaitOptions::NOHANG) {}
}

fn main() {
    let pid = getpid();
    eprintln!("============================================");
    eprintln!("  MobileOS init (PID {})", pid.as_raw_nonzero());
    eprintln!("============================================");

    mount_filesystems();

    let shell_pid = spawn_shell();

    if shell_pid.is_none() {
        eprintln!("[init] no shell available, halting");
        loop {
            reap_zombies();
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }

    // PID 1 must never exit — reap children forever
    loop {
        match waitpid(None, WaitOptions::empty()) {
            Ok(Some((pid, status))) => {
                eprintln!("[init] child {} exited: {:?}", pid.as_raw_nonzero(), status);
            }
            _ => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        reap_zombies();
    }
}
