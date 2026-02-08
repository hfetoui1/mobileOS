// ABOUTME: Service configuration parsing for the init system.
// ABOUTME: Reads TOML service files and produces typed ServiceConfig values.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestartPolicy {
    Always,
    #[default]
    OnFailure,
    Never,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServiceType {
    #[default]
    Simple,
    Oneshot,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServiceConfig {
    pub name: String,
    pub exec: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub restart: RestartPolicy,
    #[serde(default)]
    pub service_type: ServiceType,
    #[serde(default)]
    pub environment: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct ServiceFile {
    service: ServiceConfig,
}

pub fn parse_service(toml_str: &str) -> Result<ServiceConfig> {
    let file: ServiceFile = toml::from_str(toml_str)
        .context("failed to parse service config")?;
    Ok(file.service)
}

pub fn load_services_from_dir(dir: &Path) -> Result<Vec<ServiceConfig>> {
    let mut services = Vec::new();

    if !dir.exists() {
        return Ok(services);
    }

    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("failed to read service directory: {}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "toml")
        })
        .collect();

    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let config = parse_service(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        services.push(config);
    }

    Ok(services)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_service() {
        let toml = r#"
            [service]
            name = "console"
            exec = "/bin/sh"
        "#;

        let svc = parse_service(toml).unwrap();
        assert_eq!(svc.name, "console");
        assert_eq!(svc.exec, "/bin/sh");
        assert!(svc.args.is_empty());
        assert!(svc.depends_on.is_empty());
        assert_eq!(svc.restart, RestartPolicy::OnFailure);
        assert_eq!(svc.service_type, ServiceType::Simple);
        assert!(svc.environment.is_empty());
    }

    #[test]
    fn parse_full_service() {
        let toml = r#"
            [service]
            name = "compositor"
            exec = "/usr/bin/mos-compositor"
            args = ["--backend", "drm"]
            depends_on = ["udevd", "dbus"]
            restart = "always"
            service_type = "simple"

            [service.environment]
            XDG_RUNTIME_DIR = "/run"
            WAYLAND_DISPLAY = "wayland-0"
        "#;

        let svc = parse_service(toml).unwrap();
        assert_eq!(svc.name, "compositor");
        assert_eq!(svc.exec, "/usr/bin/mos-compositor");
        assert_eq!(svc.args, vec!["--backend", "drm"]);
        assert_eq!(svc.depends_on, vec!["udevd", "dbus"]);
        assert_eq!(svc.restart, RestartPolicy::Always);
        assert_eq!(svc.service_type, ServiceType::Simple);
        assert_eq!(svc.environment.get("XDG_RUNTIME_DIR").unwrap(), "/run");
    }

    #[test]
    fn parse_oneshot_service() {
        let toml = r#"
            [service]
            name = "hostname"
            exec = "/bin/hostname"
            args = ["mobileos"]
            restart = "never"
            service_type = "oneshot"
        "#;

        let svc = parse_service(toml).unwrap();
        assert_eq!(svc.service_type, ServiceType::Oneshot);
        assert_eq!(svc.restart, RestartPolicy::Never);
    }

    #[test]
    fn parse_invalid_toml_fails() {
        let toml = "this is not valid toml {{{{";
        assert!(parse_service(toml).is_err());
    }

    #[test]
    fn parse_missing_required_fields_fails() {
        let toml = r#"
            [service]
            name = "broken"
        "#;
        assert!(parse_service(toml).is_err());
    }

    #[test]
    fn load_from_directory() {
        let dir = tempfile::tempdir().unwrap();

        std::fs::write(
            dir.path().join("01-console.toml"),
            r#"
                [service]
                name = "console"
                exec = "/bin/sh"
            "#,
        )
        .unwrap();

        std::fs::write(
            dir.path().join("02-logger.toml"),
            r#"
                [service]
                name = "logger"
                exec = "/usr/bin/logger"
                depends_on = ["console"]
            "#,
        )
        .unwrap();

        // Non-toml file should be ignored
        std::fs::write(dir.path().join("readme.txt"), "ignore me").unwrap();

        let services = load_services_from_dir(dir.path()).unwrap();
        assert_eq!(services.len(), 2);
        assert_eq!(services[0].name, "console");
        assert_eq!(services[1].name, "logger");
    }

    #[test]
    fn load_from_nonexistent_dir_returns_empty() {
        let services = load_services_from_dir(Path::new("/nonexistent/path")).unwrap();
        assert!(services.is_empty());
    }
}
