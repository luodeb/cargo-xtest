use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::types::{CargoToml, DefConfig, ProjectConfig};

/// Locate the workspace root by searching upward from CWD for `defconfig.toml`.
pub fn project_root() -> PathBuf {
    let mut dir = std::env::current_dir().expect("cannot determine current directory");
    loop {
        if dir.join("defconfig.toml").exists() {
            return dir;
        }
        if !dir.pop() {
            panic!("could not find defconfig.toml in any parent directory");
        }
    }
}

/// Parse `defconfig.toml` and return the xconfig definitions.
pub fn load_defconfig(root: &Path) -> Result<std::collections::HashMap<String, crate::types::XConfigDef>> {
    let path = root.join("defconfig.toml");
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("read {}", path.display()))?;
    let def: DefConfig = toml::from_str(&content).context("parse defconfig.toml")?;
    Ok(def.xconfig.unwrap_or_default())
}

/// Ensure `.config.toml` exists. If missing, generate from `defconfig.toml` defaults.
pub fn ensure_config_toml(root: &Path) -> Result<()> {
    let config_path = root.join(".config.toml");
    if config_path.exists() {
        return Ok(());
    }

    let defs = load_defconfig(root)?;
    let mut lines = vec!["# Auto-generated from defconfig.toml — edit as needed.".to_string()];
    lines.push("[xconfig]".to_string());

    // Sort keys for deterministic output
    let mut keys: Vec<&String> = defs.keys().collect();
    keys.sort();
    for key in keys {
        let def = &defs[key];
        if let Some(desc) = &def.description {
            lines.push(format!("# {desc}"));
        }
        lines.push(format!("{} = {}", key, def.default));
    }
    lines.push(String::new()); // trailing newline

    let content = lines.join("\n");
    std::fs::write(&config_path, &content)?;
    eprintln!("[xbuild] generated .config.toml from defconfig.toml");
    Ok(())
}

/// Validate `.config.toml` values against `defconfig.toml` type definitions.
/// Reports unknown keys, missing keys, and type mismatches.
fn validate_config(
    config_map: &HashMap<String, toml::Value>,
    defs: &HashMap<String, crate::types::XConfigDef>,
) -> Result<()> {
    let mut errors: Vec<String> = Vec::new();

    // Check for unknown keys (in .config.toml but not in defconfig.toml)
    for key in config_map.keys() {
        if !defs.contains_key(key) {
            errors.push(format!("unknown xconfig key `{key}` (not defined in defconfig.toml)"));
        }
    }

    // Check for missing keys and type mismatches
    for (key, def) in defs {
        match config_map.get(key) {
            None => {
                errors.push(format!(
                    "missing xconfig key `{key}` (defined in defconfig.toml as type=\"{}\")",
                    def.typ
                ));
            }
            Some(val) => {
                let type_ok = match def.typ.as_str() {
                    "bool" => val.is_bool(),
                    "int" => val.is_integer(),
                    "string" => val.is_str(),
                    other => {
                        errors.push(format!(
                            "xconfig key `{key}`: unsupported type `{other}` in defconfig.toml"
                        ));
                        continue;
                    }
                };
                if !type_ok {
                    errors.push(format!(
                        "xconfig key `{key}`: expected type `{}`, got `{val}`",
                        def.typ
                    ));
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        for e in &errors {
            eprintln!("[xbuild] error: {e}");
        }
        anyhow::bail!(
            ".config.toml validation failed ({} error{})",
            errors.len(),
            if errors.len() == 1 { "" } else { "s" }
        );
    }
}

/// Read `.config.toml` and return (active_keys, all_keys).
/// `all_keys` is derived from `defconfig.toml` (authoritative list).
/// Validates value types against `defconfig.toml` definitions.
pub fn load_active_xconfigs(root: &Path) -> Result<(Vec<String>, Vec<String>)> {
    // all_keys comes from defconfig.toml — the authoritative source
    let defs = load_defconfig(root)?;
    let all_keys: Vec<String> = defs.keys().cloned().collect();

    let config_path = root.join(".config.toml");
    let config_str = std::fs::read_to_string(&config_path)
        .with_context(|| format!("read {}", config_path.display()))?;
    let config: ProjectConfig = toml::from_str(&config_str).context("parse .config.toml")?;

    let map = config.xconfig.unwrap_or_default();

    // Validate against defconfig.toml
    validate_config(&map, &defs)?;

    let active: Vec<String> = map
        .into_iter()
        .filter(|(_, v)| v.as_bool().unwrap_or(false))
        .map(|(k, _)| k)
        .collect();

    Ok((active, all_keys))
}

/// Scan a `Cargo.toml` for `[package.metadata.xconfig]`.
/// Populates `feature_map`: target_crate → Vec<feature_name>
///
/// Spec format:
///   - `"crate_name/feature"` → enable feature on another crate
///   - `"feature"` (no slash) → enable feature on self (this crate)
pub fn collect_xconfig_metadata(
    cargo_toml: &Path,
    active: &[String],
    feature_map: &mut HashMap<String, Vec<String>>,
) -> Result<()> {
    let content = std::fs::read_to_string(cargo_toml)?;
    let parsed: CargoToml =
        toml::from_str(&content).with_context(|| format!("parse {}", cargo_toml.display()))?;

    let pkg = match parsed.package {
        Some(p) => p,
        None => return Ok(()),
    };

    let self_name = pkg.name.clone();
    let xconfig = match pkg.metadata.and_then(|m| m.xconfig) {
        Some(x) => x,
        None => return Ok(()),
    };

    for key in active {
        if let Some(specs) = xconfig.get(key) {
            for spec in specs {
                if let Some((crate_name, feature)) = spec.split_once('/') {
                    // "crate_name/feature" → enable feature on another crate
                    feature_map
                        .entry(crate_name.to_string())
                        .or_default()
                        .push(feature.to_string());
                } else if let Some(ref name) = self_name {
                    // "feature" → enable feature on self
                    feature_map
                        .entry(name.clone())
                        .or_default()
                        .push(spec.clone());
                }
            }
        }
    }
    Ok(())
}

/// Walk `crates/` and top-level packages to collect the full feature_map.
pub fn collect_all_metadata(
    root: &Path,
    active: &[String],
) -> Result<HashMap<String, Vec<String>>> {
    let mut feature_map: HashMap<String, Vec<String>> = HashMap::new();

    let crates_dir = root.join("crates");
    if crates_dir.is_dir() {
        for entry in std::fs::read_dir(&crates_dir)? {
            let path = entry?.path();
            let toml_path = path.join("Cargo.toml");
            if toml_path.exists() {
                collect_xconfig_metadata(&toml_path, active, &mut feature_map)?;
            }
        }
    }
    for name in ["entry"] {
        let toml_path = root.join(name).join("Cargo.toml");
        if toml_path.exists() {
            collect_xconfig_metadata(&toml_path, active, &mut feature_map)?;
        }
    }

    Ok(feature_map)
}

/// Regenerate `.cargo/config.toml` so that rust-analyzer picks up
/// the active xconfig cfgs via `[build] rustflags`.
/// Also includes `--extern` and `-Ldependency` for xdeps rlibs so
/// rust-analyzer can resolve optional deps injected via RUSTC_WRAPPER.
pub fn sync_cargo_config(
    root: &Path,
    active: &[String],
    all_keys: &[String],
    rlib_paths: &HashMap<String, String>,
) -> Result<()> {
    let mut content = String::from("\
# Auto-generated by cargo-xbuild — do not edit manually.\n\
# Run `cargo xbuild` to regenerate after changing .config.toml.\n\
");

    let mut flags: Vec<String> = Vec::new();
    // --cfg for active keys only
    for c in active {
        flags.push(format!("\"--cfg={}\"" , c.to_uppercase()));
    }
    // --check-cfg for ALL known keys (so rust-analyzer never warns)
    for c in all_keys {
        flags.push(format!("\"--check-cfg=cfg({})\"", c.to_uppercase()));
    }
    flags.push("\"--check-cfg=cfg(__xfp,values(any()))\"".to_string());

    // --extern for xdeps rlibs (so RA can resolve injected optional deps)
    for (name, path) in rlib_paths {
        flags.push(format!("\"--extern={}={}\"", name, path));
    }
    // -Ldependency so RA can find transitive xdeps rlibs
    if let Some(first_rlib) = rlib_paths.values().next() {
        if let Some(deps_dir) = Path::new(first_rlib).parent() {
            flags.push(format!("\"-Ldependency={}\"", deps_dir.display()));
        }
    }

    content.push_str(&format!(
        "\n[build]\nrustflags = [\n    {}\n]\n",
        flags.join(", \n    ")
    ));

    let config_path = root.join(".cargo").join("config.toml");
    let existing = std::fs::read_to_string(&config_path).unwrap_or_default();
    if existing != content {
        std::fs::create_dir_all(root.join(".cargo"))?;
        std::fs::write(&config_path, &content)?;
        eprintln!("[xbuild] synced .cargo/config.toml");
    }
    Ok(())
}

/// Regenerate `.vscode/settings.json` so rust-analyzer picks up xconfig cfgs
/// and feature activation inferred from `[package.metadata.xconfig]`.
pub fn sync_vscode_settings(
    root: &Path,
    active: &[String],
    feature_map: &HashMap<String, Vec<String>>,
) -> Result<()> {
    use serde_json::json;
    use std::collections::BTreeSet;

    let cfgs = active
        .iter()
        .map(|c| c.to_uppercase())
        .collect::<Vec<_>>();

    let mut features = BTreeSet::new();
    for (crate_name, feats) in feature_map {
        for feat in feats {
            features.insert(format!("{crate_name}/{feat}"));
        }
    }

    let settings = json!({
        "rust-analyzer.cargo.features": features.into_iter().collect::<Vec<_>>(),
        "rust-analyzer.cargo.cfgs": cfgs,
    });

    let content = serde_json::to_string_pretty(&settings)? + "\n";
    let settings_path = root.join(".vscode").join("settings.json");
    let existing = std::fs::read_to_string(&settings_path).unwrap_or_default();
    if existing != content {
        std::fs::create_dir_all(root.join(".vscode"))?;
        std::fs::write(&settings_path, content)?;
        eprintln!("[xbuild] synced .vscode/settings.json");
    }

    Ok(())
}
