use serde::Deserialize;
use std::collections::HashMap;

/// `defconfig.toml` schema — defines all xconfig keys with metadata.
#[derive(Deserialize)]
pub struct DefConfig {
    pub xconfig: Option<HashMap<String, XConfigDef>>,
}

/// A single xconfig definition entry in `defconfig.toml`.
#[derive(Deserialize, Clone)]
pub struct XConfigDef {
    /// Human-readable description of this config switch
    #[serde(default)]
    pub description: Option<String>,
    /// Value type (currently only "bool"), reserved for future extension
    #[serde(rename = "type", default = "default_type")]
    pub typ: String,
    /// Default value when generating .config.toml
    #[serde(default)]
    pub default: bool,
}

fn default_type() -> String {
    "bool".to_string()
}

/// `.config.toml` schema — uses `toml::Value` for type validation.
#[derive(Deserialize)]
pub struct ProjectConfig {
    pub xconfig: Option<HashMap<String, toml::Value>>,
}

/// Partial `Cargo.toml` – for reading `[package.metadata.xconfig]`
#[derive(Deserialize)]
pub struct CargoToml {
    pub package: Option<Package>,
}

#[derive(Deserialize)]
pub struct Package {
    #[allow(dead_code)]
    pub name: Option<String>,
    pub metadata: Option<Metadata>,
}

#[derive(Deserialize)]
pub struct Metadata {
    pub xconfig: Option<HashMap<String, Vec<String>>>,
}

/// Partial `Cargo.toml` – for reading `[features]` of a dependency
#[derive(Deserialize)]
pub struct DepCargoToml {
    pub features: Option<HashMap<String, Vec<String>>>,
}

/// A single JSON line from `cargo build --message-format=json`
#[derive(Deserialize)]
pub struct CargoMessage {
    pub reason: String,
    #[serde(default)]
    pub target: Option<CargoTarget>,
    #[serde(default)]
    pub filenames: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct CargoTarget {
    pub name: String,
}

/// A package entry from `cargo metadata`
#[derive(Deserialize)]
pub struct CargoMetadata {
    pub packages: Vec<MetadataPackage>,
}

#[derive(Deserialize)]
pub struct MetadataPackage {
    pub name: String,
    pub manifest_path: String,
    #[serde(default)]
    pub dependencies: Vec<MetadataDep>,
}

#[derive(Deserialize)]
pub struct MetadataDep {
    pub name: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub path: Option<String>,
    /// Version requirement string, e.g. "^1.0"
    #[serde(default)]
    pub req: Option<String>,
    /// Features enabled on this dep
    #[serde(default)]
    pub features: Vec<String>,
    /// Whether default features are used (defaults to true)
    #[serde(default = "default_true")]
    pub uses_default_features: bool,
}

fn default_true() -> bool {
    true
}

/// Info about an optional dep that needs extern injection.
#[derive(Debug, Clone)]
pub struct ExternDep {
    /// Crate name (normalized with underscores)
    pub crate_name: String,
    /// Original package name (with hyphens)
    pub pkg_name: String,
    /// Dependency source spec for Cargo.toml
    pub source: DepSource,
}

#[derive(Debug, Clone)]
pub enum DepSource {
    Git(String),
    Path(String),
    Registry {
        version: String,
        features: Vec<String>,
        default_features: bool,
    },
}
