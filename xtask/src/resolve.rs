use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::types::*;

/// Given a feature_map (crate → features to enable), resolve the
/// optional dependencies that each feature activates by parsing the
/// target crate's `[features]` table and dependency metadata.
///
/// Returns extern_map: target_crate → Vec<ExternDep>
pub fn resolve_extern_map(
    root: &Path,
    feature_map: &HashMap<String, Vec<String>>,
) -> Result<HashMap<String, Vec<ExternDep>>> {
    if feature_map.is_empty() {
        return Ok(HashMap::new());
    }

    // Try --no-deps first, fall back to full if needed
    let try_metadata = |extra: &[&str]| -> Result<Vec<u8>> {
        let mut args = vec!["metadata", "--format-version=1"];
        args.extend_from_slice(extra);
        let output = Command::new("cargo")
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(root)
            .output()
            .context("cargo metadata")?;
        if !output.status.success() {
            bail!(
                "cargo metadata failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(output.stdout)
    };

    // Full metadata (includes transitive deps like git crates)
    let metadata_bytes = match try_metadata(&["--no-deps"]) {
        Ok(bytes) => {
            let meta: CargoMetadata = serde_json::from_slice(&bytes)?;
            let has_all = feature_map
                .keys()
                .all(|k| meta.packages.iter().any(|p| &p.name == k));
            if has_all {
                bytes
            } else {
                try_metadata(&[])?
            }
        }
        Err(_) => try_metadata(&[])?,
    };

    resolve_extern_map_from_metadata(&metadata_bytes, feature_map)
}

fn resolve_extern_map_from_metadata(
    metadata_json: &[u8],
    feature_map: &HashMap<String, Vec<String>>,
) -> Result<HashMap<String, Vec<ExternDep>>> {
    let meta: CargoMetadata =
        serde_json::from_slice(metadata_json).context("parse cargo metadata")?;

    let pkg_lookup: HashMap<String, &MetadataPackage> =
        meta.packages.iter().map(|p| (p.name.clone(), p)).collect();

    let mut extern_map: HashMap<String, Vec<ExternDep>> = HashMap::new();

    for (crate_name, features) in feature_map {
        let pkg = match pkg_lookup.get(crate_name) {
            Some(p) => p,
            None => continue,
        };

        // Parse [features] table from the manifest
        let content = std::fs::read_to_string(&pkg.manifest_path)
            .with_context(|| format!("read {}", pkg.manifest_path))?;
        let dep_toml: DepCargoToml =
            toml::from_str(&content).with_context(|| format!("parse {}", pkg.manifest_path))?;

        let feat_table = match &dep_toml.features {
            Some(f) => f,
            None => continue,
        };

        // Build dep name → source lookup from metadata dependencies
        let dep_source_lookup: HashMap<String, DepSource> = pkg
            .dependencies
            .iter()
            .filter(|d| d.optional)
            .map(|d| {
                let source = if let Some(src) = &d.source {
                    if src.starts_with("git+") {
                        let url = src.strip_prefix("git+").unwrap();
                        let url = url.split('#').next().unwrap_or(url);
                        DepSource::Git(url.to_string())
                    } else {
                        DepSource::Registry
                    }
                } else if let Some(path) = &d.path {
                    DepSource::Path(path.clone())
                } else {
                    DepSource::Registry
                };
                (d.name.clone(), source)
            })
            .collect();

        for feat_name in features {
            if let Some(activates) = feat_table.get(feat_name) {
                for entry in activates {
                    if let Some(dep_name) = entry.strip_prefix("dep:") {
                        let normalized = dep_name.replace('-', "_");
                        let source = dep_source_lookup
                            .get(dep_name)
                            .cloned()
                            .unwrap_or(DepSource::Registry);
                        extern_map.entry(crate_name.clone()).or_default().push(
                            ExternDep {
                                crate_name: normalized,
                                pkg_name: dep_name.to_string(),
                                source,
                            },
                        );
                    }
                }
            }
        }
    }

    Ok(extern_map)
}
