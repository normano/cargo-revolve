use crate::config::RevolveConfig;
use crate::error::Result;
use anyhow::{anyhow, Context};
use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;

// Declare all our new modules
mod commands;
mod config;
mod definitions;
mod error;

// =================================================================================================
// Command-Line Interface Definition
// =================================================================================================

#[derive(Parser, Debug)]
#[command(
  name = "cargo",
  bin_name = "cargo",
  author,
  version,
  about = "A Cargo subcommand to build RPMs using a .spec template."
)]
struct CargoCli {
    #[command(subcommand)]
    command: CargoCommands,
}

// This enum exists solely to capture the "revolve" subcommand.
#[derive(Subcommand, Debug)]
enum CargoCommands {
    Revolve(RevolveCli),
}

// This is our ACTUAL application's CLI definition.
#[derive(Parser, Debug)]
struct RevolveCli {
  #[command(subcommand)]
  command: Commands,

  #[arg(short, long, action = clap::ArgAction::Count, global = true)]
  verbose: u8,
}

#[derive(Subcommand, Debug)]
enum Commands {
  /// Build an RPM package from a .spec template.
  Build {
    /// Perform all steps except the final `rpmbuild` execution.
    /// This will print the rendered .spec and the command to be run.
    #[arg(long)]
    dry_run: bool,

    /// Build directly from the source tree without creating a source archive.
    /// This requires a .spec file that does not use %setup.
    #[arg(long)]
    no_archive: bool,

    /// After building, verify the RPM contents against the Cargo.toml configuration.
    #[arg(long)]
    verify: bool,
  },
  /// Display detailed information about an RPM file.
  Info {
    /// The path to the .rpm file to inspect.
    #[arg(required = true)]
    rpm_file: PathBuf,
  },
}

// =================================================================================================
// Configuration Loading Helpers
// =================================================================================================

// This struct represents the `[package.metadata]` table
#[derive(serde::Deserialize, Debug)]
struct MetadataToml {
  #[serde(rename = "revolve")]
  revolve_config: Option<RevolveConfig>,
}

// This struct represents the `[package]` table
#[derive(serde::Deserialize, Debug)]
struct PackageToml {
  metadata: Option<MetadataToml>,
}

// THIS IS THE NEW TOP-LEVEL STRUCT.
// It represents the entire Cargo.toml file, which has a `[package]` table.
#[derive(serde::Deserialize, Debug)]
struct Manifest {
    package: PackageToml,
}

// =================================================================================================
// Main Application Logic
// =================================================================================================

fn main() -> Result<()> {
  // 1. Parse Command-Line Arguments
  let CargoCli { command: CargoCommands::Revolve(cli) } = CargoCli::parse();

  // 2. Initialize Logging (globally, based on verbose flag)
  let log_level = match cli.verbose {
    0 => "warn",
    1 => "info",
    2 => "debug",
    _ => "trace",
  };
  env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level)).init();
  log::debug!("CLI arguments parsed: {:?}", cli);

  // 3. Load Project Configuration
  log::info!("Loading project configuration...");
  let metadata = cargo_metadata::MetadataCommand::new()
    .exec()
    .context("Failed to execute `cargo metadata`")?;

  let root_package = metadata
    .root_package()
    .ok_or_else(|| anyhow!("Could not find root package in workspace"))?;

  let manifest_path = &root_package.manifest_path;
  log::debug!("Found manifest path: {}", manifest_path);

  // 4. Dispatch to the appropriate command
  match cli.command {
    Commands::Build {
      dry_run,
      no_archive,
      verify,
    } => {
      let revolve_config = load_revolve_config(root_package.manifest_path.as_std_path())?;
      log::debug!(
        "Dispatching to 'build' command with dry_run={}, no_archive={}",
        dry_run,
        no_archive
      );
      commands::build::run(&revolve_config, root_package, dry_run, no_archive, verify)?;
    }
    Commands::Info { rpm_file } => {
      log::debug!(
        "Dispatching to 'info' command for file: {}",
        rpm_file.display()
      );
      // The info command doesn't need project config, so we create a new module for it.
      commands::info::run(&rpm_file)?;
    }
  }

  Ok(())
}

fn load_revolve_config(manifest_path: &std::path::Path) -> Result<RevolveConfig> {
  let manifest_content = fs::read_to_string(manifest_path)
    .with_context(|| format!("Failed to read manifest file at {}", manifest_path.display()))?;

  // Parse into the new, correct top-level struct
  let manifest: Manifest =
    toml::from_str(&manifest_content).context("Failed to parse Cargo.toml")?;
    
  // Now, drill down through the correct structure
  manifest
    .package
    .metadata
    .and_then(|m| m.revolve_config)
    .ok_or_else(|| {
      anyhow!(
        "Missing `[package.metadata.revolve]` table in {}",
        manifest_path.display()
      )
    })
}