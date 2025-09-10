use crate::config::RevolveConfig;
use crate::definitions::{BuilderContext, PkgContext, TemplateContext};
use crate::error::Result;

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

use anyhow::{Context, bail};
use cargo_metadata::Package as CargoPackage;
use flate2::Compression;
use flate2::write::GzEncoder;
use rpm::Package as RpmPackage;
use tar::Builder;
use tera::Tera;

/// The main entry point for the `build` command.
pub fn run(
  config: &RevolveConfig,
  package: &CargoPackage,
  target_dir: &Path,
  dry_run: bool,
  no_archive: bool,
  verify: bool,
) -> Result<()> {
  // 1. Environment Check
  check_environment()?;

  cargo_build(package, &config.build_flags, target_dir)?;

  // 2. Prepare build directories

  let manifest_dir = package.manifest_path.parent().unwrap().as_std_path();
  let revolve_dir = manifest_dir.join("target/revolve");
  let build_dir = revolve_dir.join("build");
  let rpmbuild_dir = revolve_dir.join("rpmbuild");

  fs::create_dir_all(&build_dir).with_context(|| {
    format!(
      "Failed to create build directory at {}",
      build_dir.display()
    )
  })?;

  // 4. Create source archive
  let source_archive_path = if !no_archive {
    Some(create_artifact_archive(
      config, package, target_dir, dry_run,
    )?)
  } else {
    None
  };

  let (rendered_spec_path, rendered_spec_content) = render_spec(config, package, &build_dir)?;

  if dry_run {
    println!("--- Dry Run Activated ---");
    println!(
      "\n[1/2] Rendered .spec file would be written to: {}",
      rendered_spec_path.display()
    );
    println!("----------------------------------------------------");
    println!("{}", rendered_spec_content);
    println!("----------------------------------------------------");

    let rpmbuild_command = if let Some(archive_path) = &source_archive_path {
      format!(
        "rpmbuild -ta {} --specfile {} --define='_topdir {}'",
        archive_path.display(),
        rendered_spec_path.display(),
        rpmbuild_dir.display()
      )
    } else {
      format!(
        "rpmbuild -bb {} --define='_topdir {}' --define='_sourcedir {}'",
        rendered_spec_path.display(),
        rpmbuild_dir.display(),
        manifest_dir.display() // Tell rpmbuild where to find the source
      )
    };

    println!("\n[2/2] The following `rpmbuild` command would be executed:");
    println!("{}", rpmbuild_command);
    println!("\n--- End of Dry Run ---");
  } else {
    // 5. Execute rpmbuild
    execute_rpmbuild(
      source_archive_path.as_deref(),
      &rendered_spec_path,
      &rpmbuild_dir,
      manifest_dir,
    )?;

    // 6. Collect artifacts
    let artifacts = collect_artifacts(&rpmbuild_dir, &config.output_dir, manifest_dir)?;
    if verify {
      log::info!("--verify flag is set, verifying package contents...");

      // Find the main binary RPM instead of just taking the first one.
      let expected_binary_rpm_prefix = format!("{}-{}", package.name, package.version);

      let main_binary_rpm = artifacts.iter().find(|path| {
        let filename = path.file_name().unwrap_or_default().to_string_lossy();
        filename.starts_with(&expected_binary_rpm_prefix)
          && !filename.contains("debuginfo")
          && !filename.contains("debugsource")
          && !filename.contains(".src.rpm") // Also exclude source RPMs explicitly
      });

      if let Some(rpm_path) = main_binary_rpm {
        verify_package(rpm_path, package, config)?;
      } else {
        // Provide a helpful error if we built RPMs but couldn't find the main one.
        bail!(
          "Verification failed: Could not find the main binary RPM to verify. Found artifacts: {:?}",
          artifacts
        );
      }
    }
  }

  Ok(())
}

fn check_environment() -> Result<()> {
  log::info!("Checking for 'rpmbuild' executable...");
  which::which("rpmbuild").context(
    "'rpmbuild' command not found. Please ensure it is installed and in your system's PATH.",
  )?;
  log::info!("'rpmbuild' found.");
  Ok(())
}

fn render_spec(
  config: &RevolveConfig,
  package: &CargoPackage,
  build_dir: &Path,
) -> Result<(PathBuf, String)> {
  log::info!("Rendering .spec template...");
  let manifest_dir = package.manifest_path.parent().unwrap().as_std_path();
  let template_path = manifest_dir.join(&config.spec_template);

  // Read changelog content if configured.
  let changelog_content = if let Some(changelog_file) = &config.changelog {
    let changelog_path = manifest_dir.join(changelog_file);
    log::info!("Reading changelog from {}", changelog_path.display());
    match fs::read_to_string(&changelog_path) {
      Ok(content) => Some(content),
      Err(e) => {
        // A missing changelog is not a fatal error; warn the user and continue.
        log::warn!(
          "Failed to read changelog file at {}: {}",
          changelog_path.display(),
          e
        );
        None
      }
    }
  } else {
    None
  };

  let mut tera = Tera::default();
  tera
    .add_template_file(&template_path, Some("spec"))
    .with_context(|| {
      format!(
        "Failed to load spec template from {}",
        template_path.display()
      )
    })?;

  let archive_root_dir = format!("{}-{}", package.name, package.version);

  let context = tera::Context::from_serialize(TemplateContext {
    pkg: PkgContext {
      name: &package.name,
      version: &package.version.to_string(),
      description: package.description.as_deref(),
      license: package.license.as_deref(),
    },
    builder: BuilderContext {
      spec_template: &config.spec_template,
      archive_root_dir: &archive_root_dir,
      changelog: changelog_content.as_deref(),
      assets: config.assets.as_ref(),
      build_flags: config.build_flags.as_ref(),
    },
  })?;

  let rendered = tera.render("spec", &context)?;

  let spec_filename = format!("{}-{}.spec", package.name, package.version);
  let final_spec_path = build_dir.join(spec_filename);

  fs::write(&final_spec_path, &rendered).with_context(|| {
    format!(
      "Failed to write rendered spec to {}",
      final_spec_path.display()
    )
  })?;

  log::info!(
    "Successfully rendered spec file to {}",
    final_spec_path.display()
  );

  Ok((final_spec_path, rendered))
}

fn create_artifact_archive(
  config: &RevolveConfig,
  package: &CargoPackage,
  target_dir: &Path,
  dry_run: bool,
) -> Result<PathBuf> {
  log::info!("Creating artifact archive...");

  let project_dir = package.manifest_path.parent().unwrap().as_std_path();
  let archive_filename = format!("{}-{}.tar.gz", package.name, package.version);
  let archive_path = project_dir.join("target").join(&archive_filename);

  if !dry_run {
    let gz_file = fs::File::create(&archive_path)?;
    let encoder = GzEncoder::new(gz_file, Compression::default());
    let mut builder = Builder::new(encoder);
    let archive_root_dir = format!("{}-{}", package.name, package.version);

    if let Some(assets) = &config.assets {
      for asset in assets {
        let source_path = if asset.source.starts_with("target/") {
          // This is a build artifact, resolve it from the true target directory.
          // We strip "target/" from the start of the source path.
          target_dir.join(asset.source.strip_prefix("target/").unwrap())
        } else {
          // This is a project file, resolve it from the project's own directory.
          project_dir.join(&asset.source)
        };

        if !source_path.exists() {
          bail!(
            "Asset source file not found: {}. Please run 'cargo build' first or ensure the path is correct.",
            source_path.display()
          );
        }
        // The destination inside the archive is just the filename.
        let dest_path = Path::new(&archive_root_dir).join(source_path.file_name().unwrap());
        builder.append_path_with_name(&source_path, dest_path)?;
      }
    }
    builder.into_inner()?.finish()?;
  }
  Ok(archive_path)
}

// This function will now run `cargo build`
fn cargo_build(
  package: &CargoPackage,
  build_flags: &Option<Vec<String>>,
  target_dir: &Path,
) -> Result<()> {
  log::info!("Compiling package with 'cargo build'...");
  let project_dir = package.manifest_path.parent().unwrap().as_std_path();
  let mut cmd = Command::new("cargo");
  cmd
    .arg("build")
    .current_dir(project_dir)
    .arg("--target-dir")
    .arg(target_dir)
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped());

  if let Some(flags) = build_flags {
    cmd.args(flags);
  }

  // Default to --release if no flags are provided, as it's the most common case.
  if build_flags.is_none() {
    cmd.arg("--release");
  }

  // Stream the output for better UX
  let mut child = cmd.spawn().context("Failed to spawn 'cargo build'")?;

  let stdout = child.stdout.take().unwrap();
  let stderr = child.stderr.take().unwrap();

  let stdout_thread = thread::spawn(|| {
    let reader = BufReader::new(stdout);
    for line in reader.lines() {
      println!("{}", line.unwrap());
    }
  });

  let stderr_thread = thread::spawn(|| {
    let reader = BufReader::new(stderr);
    for line in reader.lines() {
      eprintln!("{}", line.unwrap());
    }
  });

  stdout_thread.join().unwrap();
  stderr_thread.join().unwrap();

  let status = child.wait().context("Failed to wait for 'cargo build'")?;

  if !status.success() {
    bail!("'cargo build' failed with exit code: {}", status);
  }

  Ok(())
}

fn execute_rpmbuild(
  archive_path: Option<&Path>,
  spec_path: &Path, // This is the path to the spec file in our `target/revolve/build` dir
  rpmbuild_dir: &Path,
  project_root: &Path,
) -> Result<()> {
  log::info!("Executing 'rpmbuild' using compatible method...");

  let sources_dir = rpmbuild_dir.join("SOURCES");
  let specs_dir = rpmbuild_dir.join("SPECS");
  fs::create_dir_all(&sources_dir)?;
  fs::create_dir_all(&specs_dir)?;

  let spec_filename = spec_path.file_name().unwrap();
  let final_spec_path = specs_dir.join(spec_filename);
  fs::copy(spec_path, &final_spec_path).with_context(|| {
    format!(
      "Failed to copy spec file from {} to {}",
      spec_path.display(),
      final_spec_path.display()
    )
  })?;

  let mut cmd = Command::new("rpmbuild");
  let topdir_arg = format!("--define=_topdir {}", rpmbuild_dir.display());
  cmd.arg(topdir_arg);

  if let Some(archive) = archive_path {
    log::debug!("Copying source archive: {}", archive.display());
    let archive_filename = archive.file_name().unwrap();
    let final_archive_path = sources_dir.join(archive_filename);
    fs::copy(archive, &final_archive_path)?;

    // Change -ba (build all) to -bb (build binary).
    // This will only create the binary RPM, not the source RPM.
    cmd.arg("-bb").arg(&final_spec_path);
  } else {
    log::debug!("Building from source directory: {}", project_root.display());
    let sourcedir_arg = format!("--define=_sourcedir {}", project_root.display());
    cmd.arg("-bb").arg(&final_spec_path).arg(sourcedir_arg);
  }

  cmd
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped());

  let mut child = cmd.spawn().context("Failed to spawn 'rpmbuild'")?;

  let stdout = child.stdout.take().unwrap();
  let stderr = child.stderr.take().unwrap();

  let stdout_thread = thread::spawn(|| {
    let reader = BufReader::new(stdout);
    for line in reader.lines() {
      println!("{}", line.unwrap());
    }
  });

  let stderr_thread = thread::spawn(|| {
    let reader = BufReader::new(stderr);
    for line in reader.lines() {
      eprintln!("{}", line.unwrap());
    }
  });

  stdout_thread.join().unwrap();
  stderr_thread.join().unwrap();
  
  let status = child.wait().context("Failed to wait for 'rpmbuild'")?;

  if !status.success() {
    bail!("'rpmbuild' failed with exit code: {}", status);
  }

  log::info!("'rpmbuild' executed successfully.");
  Ok(())
}

// collect_artifacts now returns a list of found RPMs
fn collect_artifacts(
  rpmbuild_dir: &Path,
  output_dir: &Option<String>,
  project_root: &Path,
) -> Result<Vec<PathBuf>> {
  log::info!("Collecting build artifacts...");

  // Determine the final destination directory if output_dir is specified
  let final_output_dir = if let Some(dir_str) = output_dir {
    let dir = project_root.join(dir_str);
    log::info!(
      "Output directory set. Artifacts will be copied to {}",
      dir.display()
    );
    fs::create_dir_all(&dir)
      .with_context(|| format!("Failed to create output directory at {}", dir.display()))?;
    Some(dir)
  } else {
    None
  };

  let rpms_dir = rpmbuild_dir.join("RPMS");
  let mut found_rpms = Vec::new();

  if rpms_dir.exists() {
    // Walk the directory to find any .rpm files
    for entry in walkdir::WalkDir::new(rpms_dir) {
      let entry = entry.context("Failed to read directory entry")?;
      if entry.path().extension().map_or(false, |e| e == "rpm") {
        let source_path = entry.path();
        log::info!("Found RPM artifact: {}", source_path.display());

        // If an output directory is configured, copy the artifact there.
        if let Some(dest_dir) = &final_output_dir {
          let dest_path = dest_dir.join(source_path.file_name().unwrap());
          fs::copy(source_path, &dest_path).with_context(|| {
            format!(
              "Failed to copy artifact from {} to {}",
              source_path.display(),
              dest_path.display()
            )
          })?;
          log::info!("Copied artifact to {}", dest_path.display());
          // The final artifact path is the destination path.
          found_rpms.push(dest_path);
        } else {
          // Otherwise, the final artifact path is the original source path.
          found_rpms.push(source_path.to_path_buf());
        }
      }
    }
  }

  if found_rpms.is_empty() {
    log::warn!("No RPM files were found in the output directory.");
  } else {
    println!("Successfully built {} RPM package(s).", found_rpms.len());
  }

  Ok(found_rpms)
}

fn verify_package(
  rpm_path: &Path,
  cargo_package: &CargoPackage,
  config: &RevolveConfig,
) -> Result<()> {
  println!("Verifying {}...", rpm_path.display());

  let rpm_package = RpmPackage::open(rpm_path)
    .context("Failed to open and parse the generated RPM for verification")?;
  let metadata = &rpm_package.metadata;
  let mut issues_found = 0;

  // 1. Verify package metadata
  log::debug!("Verifying package metadata (Name, Version, etc.)...");
  if metadata.get_name()? != cargo_package.name {
    log::error!(
      "Verification failed: Name mismatch. Expected '{}', found '{}'",
      cargo_package.name,
      metadata.get_name()?
    );
    issues_found += 1;
  }
  if metadata.get_version()? != cargo_package.version.to_string() {
    log::error!(
      "Verification failed: Version mismatch. Expected '{}', found '{}'",
      cargo_package.version,
      metadata.get_version()?
    );
    issues_found += 1;
  }

  // Verify license if configured
  if let Some(expected_license) = &config.verify_license {
    let actual_license = metadata.get_license().unwrap_or("N/A");
    if actual_license != expected_license {
      log::error!(
        "Verification failed: License mismatch. Expected '{}', found '{}'",
        expected_license,
        actual_license
      );
      issues_found += 1;
    }
  }

  // Verify summary if configured
  if let Some(expected_summary) = &config.verify_summary {
    let actual_summary = metadata.get_summary().unwrap_or("N/A");
    if actual_summary != expected_summary {
      log::error!(
        "Verification failed: Summary mismatch. Expected '{}', found '{}'",
        expected_summary,
        actual_summary
      );
      issues_found += 1;
    }
  }

  // 2. Verify file manifest and permissions
  if let Some(expected_assets) = &config.assets {
    log::debug!("Verifying package file manifest and permissions...");
    // Fetch all file metadata at once and create a HashMap for efficient lookups.
    let actual_files_with_meta: std::collections::HashMap<_, _> = metadata
      .get_file_entries()?
      .into_iter()
      .map(|entry| (entry.path.clone(), entry))
      .collect();

    for asset in expected_assets {
      let expected_path = PathBuf::from(&asset.dest);
      match actual_files_with_meta.get(&expected_path) {
        None => {
          log::error!(
            "Verification failed: Expected file not found in package: {}",
            asset.dest
          );
          issues_found += 1;
        }
        Some(file_entry) => {
          // Check permissions if specified in config
          if let Some(expected_mode_str) = &asset.mode {
            let expected_mode = u16::from_str_radix(expected_mode_str, 8).with_context(|| {
              format!(
                "Invalid octal mode '{}' for asset {}",
                expected_mode_str, asset.source
              )
            })?;

            // Call .permissions() to get the underlying integer value.
            let actual_permission_bits = file_entry.mode.permissions() & 0o7777;

            if actual_permission_bits != expected_mode {
              log::error!(
                "Verification failed: Mode mismatch for file '{}'. Expected '{:o}', found '{:o}'",
                asset.dest,
                expected_mode,
                actual_permission_bits
              );
              issues_found += 1;
            }
          }
        }
      }
    }
  }

  if issues_found > 0 {
    bail!("{} verification issue(s) found.", issues_found);
  } else {
    println!("Verification successful. Package contents match configuration.");
  }

  Ok(())
}
