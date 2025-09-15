use serde::{Deserialize, Serialize};

/// Represents a single asset to be packaged, from the `assets` array.
#[derive(Debug, Serialize, Deserialize)]
pub struct Asset {
  pub source: String,
  pub dest: String,
  pub mode: Option<String>,
}

/// Represents the `build_command` which can be a single command or a sequence.
#[derive(Debug, Deserialize)]
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