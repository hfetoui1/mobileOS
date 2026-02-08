// ABOUTME: MobileOS init system (PID 1).
// ABOUTME: Mounts filesystems, starts services, and supervises the process tree.

mod config;
mod dependency;
mod logging;
mod mount;
mod service;
mod shutdown;
mod signals;

use rustix::process::getpid;
use std::path::Path;
use tracing::{error, info, warn};

const SERVICES_DIR: &str = "/etc/mos/services";

fn main() {
    logging::init();

    let pid = getpid();
    info!(pid = pid.as_raw_nonzero().get(), "MobileOS init starting");

    let signals = match signals::SignalState::register() {
        Ok(s) => s,
        Err(e) => {
            error!(error = %e, "failed to register signal handlers");
            std::process::exit(1);
        }
    };

    mount::mount_early_filesystems();

    // Create runtime dirs and set D-Bus session bus address for child services
    let _ = std::fs::create_dir_all("/run/dbus");
    // SAFETY: init is single-threaded at this point (before spawning any services)
    unsafe {
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/dbus/session_bus_socket");
    }

    let mut manager = service::ServiceManager::new();

    // Load and start services
    match config::load_services_from_dir(Path::new(SERVICES_DIR)) {
        Ok(configs) if configs.is_empty() => {
            warn!("no service configs found in {}, spawning fallback shell", SERVICES_DIR);
            let fallback = config::ServiceConfig {
                name: "console".to_string(),
                exec: "/bin/sh".to_string(),
                args: Vec::new(),
                depends_on: Vec::new(),
                restart: config::RestartPolicy::Always,
                service_type: config::ServiceType::Simple,
                environment: std::collections::HashMap::new(),
            };
            if let Err(e) = manager.start_service(fallback) {
                error!(error = %e, "failed to start fallback shell");
            }
        }
        Ok(configs) => {
            info!(count = configs.len(), "loaded service configs");

            match dependency::resolve_start_order(&configs) {
                Ok(order) => {
                    info!(order = ?order, "resolved start order");

                    let config_map: std::collections::HashMap<&str, &config::ServiceConfig> =
                        configs.iter().map(|c| (c.name.as_str(), c)).collect();

                    for name in &order {
                        if let Some(config) = config_map.get(name.as_str()) {
                            if let Err(e) = manager.start_service((*config).clone()) {
                                error!(service = %name, error = %e, "failed to start service");
                            }
                        }
                    }
                }
                Err(e) => {
                    error!(error = %e, "failed to resolve service dependencies");
                }
            }
        }
        Err(e) => {
            error!(error = %e, "failed to load service configs");
        }
    }

    info!("entering main loop");

    // Main event loop — PID 1 must never exit
    loop {
        if signals.is_shutdown_requested() {
            shutdown::perform_shutdown(&mut manager);
            // If reboot syscall fails, just loop forever
            loop {
                std::thread::sleep(std::time::Duration::from_secs(60));
            }
        }

        if signals.take_child_exited() {
            manager.reap();
        }

        if signals.take_reload_requested() {
            info!("reload requested (SIGUSR1) — not yet implemented");
        }

        // Sleep briefly to avoid busy-waiting
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
