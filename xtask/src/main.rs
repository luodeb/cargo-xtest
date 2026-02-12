use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

fn workspace_root() -> PathBuf {
    // xtask/Cargo.toml lives at <root>/xtask/Cargo.toml
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must live under workspace root")
        .to_path_buf()
}

fn read_toml(path: &Path) -> anyhow::Result<toml::Value> {
    let content = std::fs::read_to_string(path)?;
    Ok(content.parse::<toml::Value>()?)
}

fn bool_table_keys(value: &toml::Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Some(table) = value.as_table() else {
        return out;
    };
    for (key, v) in table {
        if v.as_bool() == Some(true) {
            out.insert(key.clone());
        }
    }
    out
}

fn parse_xconfig_mapping(crate_a_manifest: &toml::Value) -> BTreeMap<String, Vec<String>> {
    // Expected shape:
    // [package.metadata.xconfig]
    // smp = ["smp", "crate_b/smp"]
    let mut mapping = BTreeMap::new();

    let Some(package) = crate_a_manifest.get("package") else {
        return mapping;
    };
    let Some(metadata) = package.get("metadata") else {
        return mapping;
    };
    let Some(xconfig) = metadata.get("xconfig") else {
        return mapping;
    };
    let Some(table) = xconfig.as_table() else {
        return mapping;
    };

    for (k, v) in table {
        let Some(arr) = v.as_array() else {
            continue;
        };
        let mut items = Vec::new();
        for item in arr {
            if let Some(s) = item.as_str() {
                items.push(s.to_string());
            }
        }
        mapping.insert(k.clone(), items);
    }

    mapping
}

fn direct_dependency_names(entry_manifest: &toml::Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Some(deps) = entry_manifest.get("dependencies") else {
        return out;
    };
    let Some(table) = deps.as_table() else {
        return out;
    };
    out.extend(table.keys().cloned());
    out
}

fn split_pkg_feature(s: &str) -> Option<(&str, &str)> {
    let (pkg, feat) = s.split_once('/')?;
    if pkg.is_empty() || feat.is_empty() {
        return None;
    }
    Some((pkg, feat))
}

fn build_rustflags(enabled_xconfigs: &BTreeSet<String>) -> String {
    let mut parts: Vec<String> = Vec::new();
    for key in enabled_xconfigs {
        // Quotes are for rustc, not the shell (we're not going through a shell).
        parts.push(format!("--cfg=xconfig=\"{}\"", key));
    }
    parts.join(" ")
}

fn main() -> ExitCode {
    if let Err(err) = real_main() {
        eprintln!("xtask error: {err:#}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn real_main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let sub = args.next().unwrap_or_else(|| "run".to_string());
    if sub != "run" {
        anyhow::bail!("unknown subcommand: {sub} (expected: run)");
    }

    let root = workspace_root();
    let config = read_toml(&root.join(".config.toml"))?;
    let enabled = config
        .get("xconfig")
        .map(bool_table_keys)
        .unwrap_or_default();

    // Parse crate_a's mapping table for feature wiring.
    let crate_a_manifest = read_toml(&root.join("crates/crate_a/Cargo.toml"))?;
    let mapping = parse_xconfig_mapping(&crate_a_manifest);

    // Cargo only lets us enable namespaced features for *direct dependencies* of the package
    // we're building (`entry`). We'll compute that allow-list and filter accordingly.
    let entry_manifest = read_toml(&root.join("entry/Cargo.toml"))?;
    let entry_direct_deps = direct_dependency_names(&entry_manifest);

    // Collect features to enable (namespaced) that `cargo run -p entry` can accept.
    let mut pkgs_to_select: BTreeSet<String> = BTreeSet::new();
    let mut namespaced_features: BTreeSet<String> = BTreeSet::new();

    for key in &enabled {
        if let Some(items) = mapping.get(key) {
            for item in items {
                if let Some((pkg, feat)) = split_pkg_feature(item) {
                    // Only allow features for entry's direct dependencies.
                    if entry_direct_deps.contains(pkg) {
                        pkgs_to_select.insert(pkg.to_string());
                        namespaced_features.insert(format!("{pkg}/{feat}"));
                    }
                } else {
                    // Bare feature name: assume it's a crate_a feature.
                    if entry_direct_deps.contains("crate_a") {
                        pkgs_to_select.insert("crate_a".to_string());
                        namespaced_features.insert(format!("crate_a/{item}"));
                    }
                }
            }
        }
    }

    // Always run the entry binary. Use namespaced features to enable features of workspace crates
    // without introducing a `[features]` section in entry itself.
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&root);
    cmd.arg("run").arg("-p").arg("entry");

    // Note: `cargo run` does not accept multiple `-p`, so we rely on namespaced features.
    // (We still compute `pkgs_to_select` above to decide whether a mapping exists; it is not used
    // directly to build CLI args.)
    let _ = pkgs_to_select;

    if !namespaced_features.is_empty() {
        cmd.arg("-F")
            .arg(namespaced_features.into_iter().collect::<Vec<_>>().join(","));
    }

    // Global xconfig: inject cfgs for all crates.
    let new_flags = build_rustflags(&enabled);
    if !new_flags.is_empty() {
        let merged = match std::env::var("RUSTFLAGS") {
            Ok(existing) if !existing.trim().is_empty() => format!("{existing} {new_flags}"),
            _ => new_flags,
        };
        cmd.env("RUSTFLAGS", merged);
    }

    let status = cmd.status()?;
    if !status.success() {
        anyhow::bail!("cargo exited with status: {status}");
    }
    Ok(())
}
