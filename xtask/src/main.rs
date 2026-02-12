use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

// ── Config types ─────────────────────────────────────────────────────

/// `.config.toml` schema
#[derive(Deserialize)]
struct ProjectConfig {
    xconfig: Option<HashMap<String, bool>>,
}

/// Partial `Cargo.toml` schema – only the fields we care about.
#[derive(Deserialize)]
struct CargoToml {
    package: Option<Package>,
}

#[derive(Deserialize)]
struct Package {
    #[allow(dead_code)]
    name: Option<String>,
    metadata: Option<Metadata>,
}

#[derive(Deserialize)]
struct Metadata {
    xconfig: Option<HashMap<String, Vec<String>>>,
}

// ── Helpers ──────────────────────────────────────────────────────────

fn project_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must live inside a subdirectory of the project root")
        .to_path_buf()
}

/// Scan a single `Cargo.toml` for `[package.metadata.xconfig]` and
/// populate `feature_map` (crate_name → Vec<feature_name>).
fn collect_xconfig_metadata(
    cargo_toml: &Path,
    active: &[String],
    feature_map: &mut HashMap<String, Vec<String>>,
) -> Result<()> {
    let content = std::fs::read_to_string(cargo_toml)?;
    let parsed: CargoToml =
        toml::from_str(&content).with_context(|| format!("parse {}", cargo_toml.display()))?;

    let xconfig = parsed
        .package
        .and_then(|p| p.metadata)
        .and_then(|m| m.xconfig);

    if let Some(xconfig) = xconfig {
        for key in active {
            if let Some(feat_specs) = xconfig.get(key) {
                for spec in feat_specs {
                    // spec = "crate_name/feature_name"
                    if let Some((crate_name, feature)) = spec.split_once('/') {
                        feature_map
                            .entry(crate_name.to_string())
                            .or_default()
                            .push(feature.to_string());
                    }
                }
            }
        }
    }
    Ok(())
}

// ── Wrapper mode (RUSTC_WRAPPER) ─────────────────────────────────────
//
// Invoked by cargo as:  <wrapper> <rustc> [rustc-args …]
//
// Global `--cfg xconfig="…"` flags are delivered through RUSTFLAGS so
// that cargo can track them for cache invalidation.  The wrapper ONLY
// handles per-crate feature injection via the XCONFIG_FEATURES env var.
//   XCONFIG_FEATURES = crate_b:smp,feat2;crate_c:other

fn wrapper_main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let rustc = &args[1];
    let rustc_args = &args[2..];

    let mut cmd = Command::new(rustc);
    cmd.args(rustc_args);

    // Which crate is being compiled?
    let crate_name = rustc_args
        .windows(2)
        .find(|w| w[0] == "--crate-name")
        .map(|w| w[1].as_str());

    // Inject --cfg feature="…" only for the targeted crate
    if let (Some(name), Ok(feat_env)) = (crate_name, std::env::var("XCONFIG_FEATURES")) {
        for entry in feat_env.split(';').filter(|s| !s.is_empty()) {
            if let Some((cn, feats)) = entry.split_once(':') {
                if cn == name {
                    for f in feats.split(',').filter(|s| !s.is_empty()) {
                        cmd.arg("--cfg").arg(format!("feature=\"{f}\""));
                    }
                }
            }
        }
    }

    let status = cmd.status().context("failed to execute rustc")?;
    std::process::exit(status.code().unwrap_or(1));
}

// ── xtask mode (orchestrator) ────────────────────────────────────────

fn xtask_main() -> Result<()> {
    let root = project_root();
    let cargo_args: Vec<String> = std::env::args().skip(1).collect();

    // 1. Read .config.toml
    let config_path = root.join(".config.toml");
    let config_str = std::fs::read_to_string(&config_path)
        .with_context(|| format!("read {}", config_path.display()))?;
    let config: ProjectConfig = toml::from_str(&config_str).context("parse .config.toml")?;

    let active: Vec<String> = config
        .xconfig
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, enabled)| *enabled)
        .map(|(k, _)| k)
        .collect();

    eprintln!("[xtask] active xconfigs: {active:?}");

    // 2. Scan every crate's Cargo.toml for [package.metadata.xconfig]
    let mut feature_map: HashMap<String, Vec<String>> = HashMap::new();

    // crates/ subdirectories
    let crates_dir = root.join("crates");
    if crates_dir.is_dir() {
        for entry in std::fs::read_dir(&crates_dir)? {
            let path = entry?.path();
            let toml_path = path.join("Cargo.toml");
            if toml_path.exists() {
                collect_xconfig_metadata(&toml_path, &active, &mut feature_map)?;
            }
        }
    }

    // top-level crate directories (entry, etc.)
    for name in ["entry"] {
        let toml_path = root.join(name).join("Cargo.toml");
        if toml_path.exists() {
            collect_xconfig_metadata(&toml_path, &active, &mut feature_map)?;
        }
    }

    eprintln!("[xtask] feature injection map: {feature_map:?}");

    // 3. Encode env vars
    //    XCONFIG_FEATURES = crate_b:smp,feat2;crate_c:other  (for the wrapper)
    //    RUSTFLAGS += --cfg xconfig="smp" ...                (for cargo cache tracking)
    let features_env = feature_map
        .iter()
        .map(|(cn, fs)| format!("{cn}:{}", fs.join(",")))
        .collect::<Vec<_>>()
        .join(";");

    // Build RUSTFLAGS: append xconfig cfgs to any existing value.
    // Cargo tracks RUSTFLAGS for fingerprinting, so toggling an xconfig
    // automatically invalidates the build cache.
    let mut rustflags = std::env::var("RUSTFLAGS").unwrap_or_default();
    for c in &active {
        rustflags.push_str(&format!(" --cfg=xconfig=\"{c}\""));
    }
    // Encode feature-map hash so that metadata mapping changes also
    // invalidate the cache.
    if !features_env.is_empty() {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        features_env.hash(&mut hasher);
        let h = hasher.finish();
        rustflags.push_str(&format!(" --cfg=__xfp=\"{h:016x}\""));
    }
    let rustflags = rustflags.trim().to_string();

    // 4. The wrapper is this very binary – detected via __XCONFIG_WRAPPER env.
    let wrapper = std::env::current_exe().context("locate xtask binary")?;

    // 5. Forward remaining args to cargo
    let default_args: Vec<String> = vec!["build".into(), "-p".into(), "entry".into()];
    let args: &[String] = if cargo_args.is_empty() {
        &default_args
    } else {
        &cargo_args
    };

    eprintln!("[xtask] RUSTFLAGS={rustflags}");
    eprintln!("[xtask] running: cargo {}", args.join(" "));

    let status = Command::new("cargo")
        .args(args)
        .env("RUSTC_WRAPPER", &wrapper)
        .env("__XCONFIG_WRAPPER", "1")
        .env("RUSTFLAGS", &rustflags)
        .env("XCONFIG_FEATURES", &features_env)
        .current_dir(&root)
        .status()
        .context("cargo failed")?;

    if !status.success() {
        bail!("cargo exited with {status}");
    }
    Ok(())
}

// ── Entry point ──────────────────────────────────────────────────────

fn main() -> Result<()> {
    // When cargo uses us as RUSTC_WRAPPER it sets __XCONFIG_WRAPPER.
    if std::env::var("__XCONFIG_WRAPPER").is_ok() {
        wrapper_main()
    } else {
        xtask_main()
    }
}
