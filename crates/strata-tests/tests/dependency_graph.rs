//! TC-21-17: the dependency graph of the server crate does not contain `strata-dev`.
//! It reads `cargo metadata`, so it needs no service.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

/// The names of each package that `strata` reaches through a dependency of any kind.
fn reached_from_strata() -> HashSet<String> {
    let out = std::process::Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--all-features",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let meta: Value = serde_json::from_slice(&out.stdout).unwrap();
    let name_of: HashMap<&str, &str> = meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["id"].as_str().unwrap(), p["name"].as_str().unwrap()))
        .collect();
    let edges: HashMap<&str, Vec<&str>> = meta["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| {
            let deps = n["deps"]
                .as_array()
                .unwrap()
                .iter()
                .map(|d| d["pkg"].as_str().unwrap())
                .collect();
            (n["id"].as_str().unwrap(), deps)
        })
        .collect();
    let start = name_of
        .iter()
        .find(|(_, n)| **n == "strata")
        .map(|(id, _)| *id)
        .unwrap();
    let (mut seen, mut todo) = (HashSet::new(), vec![start]);
    while let Some(id) = todo.pop() {
        if seen.insert(id) {
            todo.extend(edges.get(id).into_iter().flatten());
        }
    }
    seen.into_iter().map(|id| name_of[id].to_owned()).collect()
}

/// TC-21-17
#[test]
fn server_does_not_depend_on_the_development_issuer() {
    let reached = reached_from_strata();
    assert!(
        reached.contains("strata-core"),
        "the walk reaches the dependencies"
    );
    assert!(!reached.contains("strata-dev"), "strata reaches strata-dev");
}
