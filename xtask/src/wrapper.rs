use anyhow::{Context, Result};
use std::process::Command;

/// RUSTC_WRAPPER mode: intercept rustc invocations to inject
/// `--cfg feature="…"` and `--extern name=/path/to/rlib`.
pub fn wrapper_main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let rustc = &args[1];
    let rustc_args = &args[2..];

    let mut cmd = Command::new(rustc);
    cmd.args(rustc_args);

    let crate_name = rustc_args
        .windows(2)
        .find(|w| w[0] == "--crate-name")
        .map(|w| w[1].as_str());

    // 1) Inject --cfg feature="…"
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

    // 2) Inject --extern name=/path/to/rlib
    if let (Some(name), Ok(extern_env)) = (crate_name, std::env::var("XCONFIG_EXTERNS")) {
        for entry in extern_env.split(';').filter(|s| !s.is_empty()) {
            if let Some((cn, ext_spec)) = entry.split_once(':') {
                if cn == name {
                    if let Some((ext_name, rlib_path)) = ext_spec.split_once('=') {
                        cmd.arg("--extern").arg(format!("{ext_name}={rlib_path}"));
                    }
                }
            }
        }
    }

    let status = cmd.status().context("failed to execute rustc")?;
    std::process::exit(status.code().unwrap_or(1));
}
