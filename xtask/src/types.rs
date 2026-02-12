use serde::Deserialize;
use std::collections::HashMap;

/// `.config.toml` schema
#[derive(Deserialize)]
pub struct ProjectConfig {
    pub xconfig: Option<HashMap<String, bool>>,
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
    Registry,
}
