// ABOUTME: Service lifecycle manager for the init system.
// ABOUTME: Spawns, tracks, and supervises child processes based on service configs.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::process::{Child, Command};
use tracing::{error, info, warn};

use crate::config::{RestartPolicy, ServiceConfig, ServiceType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    Stopped,
    Running,
    Finished,
    Failed,
}

struct RunningService {
    config: ServiceConfig,
    child: Child,
    restart_count: u32,
}

pub struct ServiceManager {
    running: HashMap<String, RunningService>,
    finished: HashMap<String, ServiceConfig>,
}

const MAX_RESTART_COUNT: u32 = 5;

impl ServiceManager {
    pub fn new() -> Self {
        Self {
            running: HashMap::new(),
            finished: HashMap::new(),
        }
    }

    pub fn start_service(&mut self, config: ServiceConfig) -> Result<()> {
        let name = config.name.clone();
        info!(service = %name, exec = %config.exec, "starting service");

        let mut cmd = Command::new(&config.exec);
        cmd.args(&config.args);

        for (key, val) in &config.environment {
            cmd.env(key, val);
        }

        let child = cmd
            .spawn()
            .with_context(|| format!("failed to start service '{}'", name))?;

        info!(service = %name, pid = child.id(), "service started");

        self.running.insert(
            name,
            RunningService {
                config,
                child,
                restart_count: 0,
            },
        );

        Ok(())
    }

    pub fn state(&self, name: &str) -> ServiceState {
        if self.running.contains_key(name) {
            ServiceState::Running
        } else if self.finished.contains_key(name) {
            ServiceState::Finished
        } else {
            ServiceState::Stopped
        }
    }

    pub fn running_count(&self) -> usize {
        self.running.len()
    }

    /// Check all running services for exits. Returns names of services that exited.
    pub fn reap(&mut self) -> Vec<String> {
        let mut exited = Vec::new();

        for (name, svc) in &mut self.running {
            match svc.child.try_wait() {
                Ok(Some(status)) => {
                    if status.success() {
                        info!(service = %name, "service exited successfully");
                    } else {
                        warn!(service = %name, status = ?status, "service exited with error");
                    }
                    exited.push((name.clone(), status.success()));
                }
                Ok(None) => {} // still running
                Err(e) => {
                    error!(service = %name, error = %e, "failed to check service status");
                }
            }
        }

        let mut exited_names = Vec::new();

        for (name, success) in exited {
            let svc = self.running.remove(&name).unwrap();
            let should_restart = match (&svc.config.restart, &svc.config.service_type) {
                (_, ServiceType::Oneshot) => false,
                (RestartPolicy::Always, _) => true,
                (RestartPolicy::OnFailure, _) => !success,
                (RestartPolicy::Never, _) => false,
            };

            if should_restart && svc.restart_count < MAX_RESTART_COUNT {
                info!(
                    service = %name,
                    restart_count = svc.restart_count + 1,
                    "restarting service"
                );
                match self.spawn_with_count(&svc.config, svc.restart_count + 1) {
                    Ok(()) => {}
                    Err(e) => {
                        error!(service = %name, error = %e, "failed to restart service");
                        self.finished.insert(name.clone(), svc.config);
                    }
                }
            } else {
                if should_restart {
                    error!(
                        service = %name,
                        max = MAX_RESTART_COUNT,
                        "service exceeded max restart count"
                    );
                }
                self.finished.insert(name.clone(), svc.config);
            }

            exited_names.push(name);
        }

        exited_names
    }

    fn spawn_with_count(&mut self, config: &ServiceConfig, restart_count: u32) -> Result<()> {
        let name = config.name.clone();

        let mut cmd = Command::new(&config.exec);
        cmd.args(&config.args);
        for (key, val) in &config.environment {
            cmd.env(key, val);
        }

        let child = cmd
            .spawn()
            .with_context(|| format!("failed to restart service '{}'", name))?;

        info!(service = %name, pid = child.id(), "service restarted");

        self.running.insert(
            name,
            RunningService {
                config: config.clone(),
                child,
                restart_count,
            },
        );

        Ok(())
    }

    /// Stop a service by sending SIGTERM, then SIGKILL after timeout.
    pub fn stop_service(&mut self, name: &str) -> Result<()> {
        if let Some(mut svc) = self.running.remove(name) {
            info!(service = %name, pid = svc.child.id(), "stopping service");

            // Send SIGTERM
            let _ = svc.child.kill();

            // Wait briefly for exit
            match svc.child.wait() {
                Ok(status) => {
                    info!(service = %name, status = ?status, "service stopped");
                }
                Err(e) => {
                    error!(service = %name, error = %e, "error waiting for service to stop");
                }
            }

            self.finished.insert(name.to_string(), svc.config);
        }

        Ok(())
    }

    /// Stop all running services.
    pub fn stop_all(&mut self) {
        let names: Vec<String> = self.running.keys().cloned().collect();
        for name in names {
            let _ = self.stop_service(&name);
        }
    }

    pub fn running_service_names(&self) -> Vec<&str> {
        self.running.keys().map(|s| s.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn simple_service(name: &str, exec: &str) -> ServiceConfig {
        ServiceConfig {
            name: name.to_string(),
            exec: exec.to_string(),
            args: Vec::new(),
            depends_on: Vec::new(),
            restart: RestartPolicy::Never,
            service_type: ServiceType::Simple,
            environment: HashMap::new(),
        }
    }

    #[test]
    fn new_manager_is_empty() {
        let mgr = ServiceManager::new();
        assert_eq!(mgr.running_count(), 0);
    }

    #[test]
    fn start_real_process() {
        let mut mgr = ServiceManager::new();
        let svc = simple_service("sleeper", "sleep");
        let mut svc = svc;
        svc.args = vec!["10".to_string()];

        mgr.start_service(svc).unwrap();
        assert_eq!(mgr.running_count(), 1);
        assert_eq!(mgr.state("sleeper"), ServiceState::Running);

        // Clean up
        mgr.stop_all();
    }

    #[test]
    fn start_nonexistent_binary_fails() {
        let mut mgr = ServiceManager::new();
        let svc = simple_service("broken", "/nonexistent/binary/path");

        assert!(mgr.start_service(svc).is_err());
        assert_eq!(mgr.running_count(), 0);
    }

    #[test]
    fn reap_detects_exit() {
        let mut mgr = ServiceManager::new();
        let svc = simple_service("quick", "true");

        mgr.start_service(svc).unwrap();
        assert_eq!(mgr.running_count(), 1);

        // Wait for the process to finish
        std::thread::sleep(std::time::Duration::from_millis(100));

        let exited = mgr.reap();
        assert_eq!(exited, vec!["quick"]);
        assert_eq!(mgr.running_count(), 0);
        assert_eq!(mgr.state("quick"), ServiceState::Finished);
    }

    #[test]
    fn restart_on_failure_triggers_for_failing_service() {
        let mut mgr = ServiceManager::new();
        let svc = ServiceConfig {
            name: "failing".to_string(),
            exec: "false".to_string(),
            args: Vec::new(),
            depends_on: Vec::new(),
            restart: RestartPolicy::OnFailure,
            service_type: ServiceType::Simple,
            environment: HashMap::new(),
        };

        mgr.start_service(svc).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));

        let exited = mgr.reap();
        // Service should have been restarted, so it's back in running
        assert!(exited.contains(&"failing".to_string()));
        assert_eq!(mgr.state("failing"), ServiceState::Running);

        // Clean up
        mgr.stop_all();
    }

    #[test]
    fn no_restart_for_successful_on_failure_policy() {
        let mut mgr = ServiceManager::new();
        let svc = ServiceConfig {
            name: "ok".to_string(),
            exec: "true".to_string(),
            args: Vec::new(),
            depends_on: Vec::new(),
            restart: RestartPolicy::OnFailure,
            service_type: ServiceType::Simple,
            environment: HashMap::new(),
        };

        mgr.start_service(svc).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));

        mgr.reap();
        // Successful exit with on-failure policy should NOT restart
        assert_eq!(mgr.state("ok"), ServiceState::Finished);
    }

    #[test]
    fn oneshot_never_restarts() {
        let mut mgr = ServiceManager::new();
        let svc = ServiceConfig {
            name: "setup".to_string(),
            exec: "true".to_string(),
            args: Vec::new(),
            depends_on: Vec::new(),
            restart: RestartPolicy::Always,
            service_type: ServiceType::Oneshot,
            environment: HashMap::new(),
        };

        mgr.start_service(svc).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));

        mgr.reap();
        // Oneshot should never restart even with Always policy
        assert_eq!(mgr.state("setup"), ServiceState::Finished);
    }

    #[test]
    fn stop_service_kills_process() {
        let mut mgr = ServiceManager::new();
        let mut svc = simple_service("long", "sleep");
        svc.args = vec!["60".to_string()];

        mgr.start_service(svc).unwrap();
        assert_eq!(mgr.state("long"), ServiceState::Running);

        mgr.stop_service("long").unwrap();
        assert_eq!(mgr.state("long"), ServiceState::Finished);
        assert_eq!(mgr.running_count(), 0);
    }

    #[test]
    fn service_environment_is_passed() {
        let mut mgr = ServiceManager::new();
        // Use env command to check that environment is set
        let svc = ServiceConfig {
            name: "envtest".to_string(),
            exec: "env".to_string(),
            args: Vec::new(),
            depends_on: Vec::new(),
            restart: RestartPolicy::Never,
            service_type: ServiceType::Oneshot,
            environment: HashMap::from([
                ("MY_VAR".to_string(), "hello".to_string()),
            ]),
        };

        mgr.start_service(svc).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        mgr.reap();
        assert_eq!(mgr.state("envtest"), ServiceState::Finished);
    }
}
