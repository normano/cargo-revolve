use crate::error::Result;
use anyhow::Context;
use rpm::Package;
use std::path::Path;

/// The main entry point for the `info` command.
pub fn run(rpm_file_path: &Path) -> Result<()> {
  println!("Inspecting: {}", rpm_file_path.display());

  let package = Package::open(rpm_file_path)
    .with_context(|| format!("Failed to open or parse RPM file at {}", rpm_file_path.display()))?;

  // Extract and print header information from `package.metadata`
  let metadata = &package.metadata;
  let name = metadata.get_name()?;
  let version = metadata.get_version()?;
  let release = metadata.get_release()?;
  let arch = metadata.get_arch()?;
  let size = metadata.get_installed_size()?;
  let license = metadata.get_license().unwrap_or("N/A");
  let summary = metadata.get_summary().unwrap_or("N/A");

  println!("\nPackage Summary:");
  println!("  Name:      {}", name);
  println!("  Version:   {}", version);
  println!("  Release:   {}", release);
  println!("  Arch:      {}", arch);
  println!("  Size:      {} bytes (installed)", size);
  println!("  License:   {}", license);
  println!("  Summary:   {}", summary);
  
  // Extract and print file list
  let file_paths = metadata.get_file_paths()?;
  println!("\nFiles ({}):", file_paths.len());
  for path in file_paths {
      println!("  {}", path.display());
  }

  Ok(())
}