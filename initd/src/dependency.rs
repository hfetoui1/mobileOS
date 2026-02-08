// ABOUTME: Dependency resolver for service startup ordering.
// ABOUTME: Topological sort that computes a valid start sequence respecting depends_on.

use anyhow::{bail, Result};
use std::collections::{HashMap, HashSet, VecDeque};

use crate::config::ServiceConfig;

/// Compute a valid start order for services using Kahn's topological sort.
/// Returns service names in the order they should be started.
pub fn resolve_start_order(services: &[ServiceConfig]) -> Result<Vec<String>> {
    let names: HashSet<&str> = services.iter().map(|s| s.name.as_str()).collect();

    // Validate all dependencies refer to known services
    for svc in services {
        for dep in &svc.depends_on {
            if !names.contains(dep.as_str()) {
                bail!(
                    "service '{}' depends on unknown service '{}'",
                    svc.name,
                    dep
                );
            }
        }
    }

    // Build adjacency list and in-degree count
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

    for svc in services {
        in_degree.entry(svc.name.as_str()).or_insert(0);
        for dep in &svc.depends_on {
            *in_degree.entry(svc.name.as_str()).or_insert(0) += 1;
            dependents
                .entry(dep.as_str())
                .or_default()
                .push(svc.name.as_str());
        }
    }

    // Start with services that have no dependencies
    let mut ready: Vec<&str> = Vec::new();
    for (name, deg) in &in_degree {
        if *deg == 0 {
            ready.push(name);
        }
    }
    ready.sort();
    let mut queue: VecDeque<&str> = ready.into_iter().collect();

    let mut order = Vec::with_capacity(services.len());

    while let Some(name) = queue.pop_front() {
        order.push(name.to_string());

        if let Some(deps) = dependents.get(name) {
            let mut ready = Vec::new();
            for &dependent in deps {
                let deg = in_degree.get_mut(dependent).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    ready.push(dependent);
                }
            }
            ready.sort();
            queue.extend(ready);
        }
    }

    if order.len() != services.len() {
        // Find the cycle participants
        let ordered: HashSet<&str> = order.iter().map(|s| s.as_str()).collect();
        let in_cycle: Vec<&str> = names.difference(&ordered).copied().collect();
        bail!(
            "circular dependency detected involving: {}",
            in_cycle.join(", ")
        );
    }

    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse_service;

    fn svc(name: &str, deps: &[&str]) -> ServiceConfig {
        let deps_toml = if deps.is_empty() {
            String::new()
        } else {
            format!(
                "depends_on = [{}]",
                deps.iter()
                    .map(|d| format!("\"{}\"", d))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };

        parse_service(&format!(
            r#"
            [service]
            name = "{name}"
            exec = "/usr/bin/{name}"
            {deps_toml}
            "#
        ))
        .unwrap()
    }

    #[test]
    fn no_services_returns_empty() {
        let order = resolve_start_order(&[]).unwrap();
        assert!(order.is_empty());
    }

    #[test]
    fn single_service_no_deps() {
        let services = vec![svc("console", &[])];
        let order = resolve_start_order(&services).unwrap();
        assert_eq!(order, vec!["console"]);
    }

    #[test]
    fn linear_chain() {
        let services = vec![
            svc("dbus", &[]),
            svc("compositor", &["dbus"]),
            svc("shell", &["compositor"]),
        ];
        let order = resolve_start_order(&services).unwrap();
        assert_eq!(order, vec!["dbus", "compositor", "shell"]);
    }

    #[test]
    fn diamond_dependency() {
        // A -> B, A -> C, B -> D, C -> D
        let services = vec![
            svc("a", &[]),
            svc("b", &["a"]),
            svc("c", &["a"]),
            svc("d", &["b", "c"]),
        ];
        let order = resolve_start_order(&services).unwrap();
        assert_eq!(order, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn independent_services_sorted_alphabetically() {
        let services = vec![
            svc("zebra", &[]),
            svc("alpha", &[]),
            svc("middle", &[]),
        ];
        let order = resolve_start_order(&services).unwrap();
        assert_eq!(order, vec!["alpha", "middle", "zebra"]);
    }

    #[test]
    fn circular_dependency_detected() {
        let services = vec![
            svc("a", &["b"]),
            svc("b", &["a"]),
        ];
        let err = resolve_start_order(&services).unwrap_err();
        assert!(err.to_string().contains("circular dependency"));
    }

    #[test]
    fn unknown_dependency_detected() {
        let services = vec![svc("console", &["nonexistent"])];
        let err = resolve_start_order(&services).unwrap_err();
        assert!(err.to_string().contains("unknown service"));
    }

    #[test]
    fn complex_graph() {
        // network depends on dbus
        // compositor depends on dbus
        // shell depends on compositor, network
        // audio depends on dbus
        let services = vec![
            svc("dbus", &[]),
            svc("network", &["dbus"]),
            svc("compositor", &["dbus"]),
            svc("shell", &["compositor", "network"]),
            svc("audio", &["dbus"]),
        ];
        let order = resolve_start_order(&services).unwrap();

        // dbus must be first
        assert_eq!(order[0], "dbus");
        // shell must be after compositor and network
        let shell_pos = order.iter().position(|s| s == "shell").unwrap();
        let comp_pos = order.iter().position(|s| s == "compositor").unwrap();
        let net_pos = order.iter().position(|s| s == "network").unwrap();
        assert!(shell_pos > comp_pos);
        assert!(shell_pos > net_pos);
    }
}
