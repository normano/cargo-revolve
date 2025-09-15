use serde::{Deserialize, Serialize};

/// Represents a single asset to be packaged, from the `assets` array.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Asset {
  pub source: String,
  pub dest: String,
  pub mode: Option<String>,
  #[serde(default = "default_mkdir")]
  pub mkdir: bool,
}

// This function provides the default value for `mkdir` to serde.
fn default_mkdir() -> bool {
    true
}

/// Represents the `build_command` which can be a single command or a sequence.
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum BuildCommand {
  Single(String),
  Sequence(Vec<String>),
}

/// Represents the `[package.metadata.revolve]` table in Cargo.toml.
#[derive(Debug, Deserialize)]
pub struct RevolveConfig {
  pub spec_template: String,
  pub output_dir: Option<String>,
  pub changelog: Option<String>,
  pub build_flags: Option<Vec<String>>,
  pub build_command: Option<BuildCommand>,
  pub assets: Option<Vec<Asset>>,
  pub verify_license: Option<String>,
  pub verify_summary: Option<String>,
}