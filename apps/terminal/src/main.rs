// ABOUTME: Terminal emulator application for MobileOS.
// ABOUTME: Opens a PTY, spawns /bin/sh, and connects it to a slint GUI.

use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Context;
use tracing::info;

slint::include_modules!();

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("starting terminal");

    let (master_fd, slave_fd) = open_pty()?;
    let child_pid = spawn_shell(&slave_fd)?;
    drop(slave_fd);

    info!(pid = child_pid, "shell spawned");

    set_nonblocking(&master_fd)?;

    let window = TerminalWindow::new()?;

    let (cmd_tx, cmd_rx) = mpsc::channel::<String>();

    window.on_command_submitted(move |text| {
        let _ = cmd_tx.send(text.to_string());
    });

    let weak = window.as_weak();
    let master_raw = master_fd.as_raw_fd();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(50), move || {
        let mut master = unsafe { std::fs::File::from_raw_fd(master_raw) };

        // Write pending commands to the shell
        while let Ok(cmd) = cmd_rx.try_recv() {
            let line = format!("{cmd}\n");
            let _ = master.write_all(line.as_bytes());
        }

        // Read available output from the shell
        let mut buf = [0u8; 4096];
        match master.read(&mut buf) {
            Ok(n) if n > 0 => {
                let text = String::from_utf8_lossy(&buf[..n]);
                if let Some(w) = weak.upgrade() {
                    let current = w.get_output();
                    w.set_output(format!("{current}{text}").into());
                }
            }
            _ => {}
        }

        // Prevent the File from closing the fd — we don't own it here
        std::mem::forget(master);
    });

    info!("terminal running");
    window.run()?;

    Ok(())
}

fn open_pty() -> anyhow::Result<(OwnedFd, OwnedFd)> {
    let master = rustix::pty::openpt(
        rustix::pty::OpenptFlags::RDWR | rustix::pty::OpenptFlags::NOCTTY,
    )
    .context("openpt failed")?;

    rustix::pty::grantpt(&master).context("grantpt failed")?;
    rustix::pty::unlockpt(&master).context("unlockpt failed")?;

    let slave_path = rustix::pty::ptsname(&master, Vec::new())
        .context("ptsname failed")?;

    let slave = rustix::fs::open(
        slave_path.as_c_str(),
        rustix::fs::OFlags::RDWR | rustix::fs::OFlags::NOCTTY,
        rustix::fs::Mode::empty(),
    )
    .context("failed to open slave PTY")?;

    Ok((master, slave))
}

fn spawn_shell(slave_fd: &OwnedFd) -> anyhow::Result<u32> {
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err(anyhow::anyhow!("fork failed"));
    }

    if pid == 0 {
        // Child process
        unsafe {
            libc::setsid();
        }

        let raw = slave_fd.as_raw_fd();

        // Make slave the controlling terminal and set as stdin/stdout/stderr
        unsafe {
            libc::ioctl(raw, libc::TIOCSCTTY, 0);
            libc::dup2(raw, 0);
            libc::dup2(raw, 1);
            libc::dup2(raw, 2);
            if raw > 2 {
                libc::close(raw);
            }
        }

        let shell = std::ffi::CString::new("/bin/sh").unwrap();
        let args = [shell.as_ptr(), std::ptr::null()];

        unsafe {
            libc::execvp(shell.as_ptr(), args.as_ptr());
            libc::_exit(1);
        }
    }

    Ok(pid as u32)
}

fn set_nonblocking(fd: &OwnedFd) -> anyhow::Result<()> {
    let raw = fd.as_raw_fd();
    let flags = unsafe { libc::fcntl(raw, libc::F_GETFL) };
    if flags < 0 {
        return Err(anyhow::anyhow!("fcntl F_GETFL failed"));
    }
    let ret = unsafe { libc::fcntl(raw, libc::F_SETFL, flags | libc::O_NONBLOCK) };
    if ret < 0 {
        return Err(anyhow::anyhow!("fcntl F_SETFL failed"));
    }
    Ok(())
}
