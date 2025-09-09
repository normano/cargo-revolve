use crate::config::RevolveConfig;
use crate::definitions::{BuilderContext, PkgContext, TemplateContext};
use crate::error::Result;
use anyhow::{bail, Context};
use cargo_metadata::Package as CargoPackage;
use rpm::Package as RpmPackage;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tera::Tera;

/// The main entry point for the `build` command.
pub fn run(
  config: &RevolveConfig,
  package: &CargoPackage,
  dry_run: bool,
  no_archive: bool,
  verify: bool,
) -> Result<()> {
  // 1. Environment Check
  check_environment()?;

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

  let (rendered_spec_path, rendered_spec_content) = render_spec(config, package, &build_dir)?;

  // 4. Create source archive
  let source_archive_path = if !no_archive {
    Some(create_source_archive(package, dry_run)?)
  } else {
    log::info!("--no-archive specified, skipping 'cargo package'.");
    None
  };

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
    let artifacts = collect_artifacts(&rpmbuild_dir)?;
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
  // CONVERT Utf8PathBuf to PathBuf here
  let manifest_dir = package.manifest_path.parent().unwrap().as_std_path();
  let template_path = manifest_dir.join(&config.spec_template);

  let mut tera = Tera::default();
  tera
    .add_template_file(&template_path, Some("spec"))
    .with_context(|| {
      format!(
        "Failed to load spec template from {}",
        template_path.display()
      )
    })?;

  let context = tera::Context::from_serialize(TemplateContext {
    pkg: PkgContext {
      name: &package.name,
      version: &package.version.to_string(),
      description: package.description.as_deref(),
      license: package.license.as_deref(),
    },
    builder: BuilderContext {
      spec_template: &config.spec_template,
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

fn create_source_archive(package: &CargoPackage, dry_run: bool) -> Result<PathBuf> {
  log::info!("Creating source archive with 'cargo package'...");
  let project_dir = package.manifest_path.parent().unwrap().as_std_path();
  let crate_filename = format!("{}-{}.crate", package.name, package.version);
  let crate_path = project_dir.join("target/package").join(crate_filename);

  if !dry_run {
    let output = Command::new("cargo")
      .arg("package")
      .arg("--allow-dirty")
      .current_dir(project_dir)
      .output()
      .context("Failed to execute 'cargo package'")?;

    if !output.status.success() {
      anyhow::bail!(
        "'cargo package' failed:\n--- stdout\n{}\n--- stderr\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
      );
    }

    // THE FIX: Move the existence check inside the block that creates the file.
    if !crate_path.exists() {
      anyhow::bail!(
        "Expected to find source archive at {}, but it does not exist.",
        crate_path.display()
      );
    }
  } else {
    log::info!("Dry run: skipping 'cargo package' execution.");
  }

  // The function still returns the calculated path, which is correct for a dry run.
  log::info!("Source archive path is {}", crate_path.display());
  Ok(crate_path)
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

    // THE FIX: Change -ba (build all) to -bb (build binary).
    // This will only create the binary RPM, not the source RPM.
    cmd.arg("-bb").arg(&final_spec_path);
  } else {
    log::debug!("Building from source directory: {}", project_root.display());
    let sourcedir_arg = format!("--define=_sourcedir {}", project_root.display());
    cmd.arg("-bb").arg(&final_spec_path).arg(sourcedir_arg);
  }

  let output = cmd.output().context("Failed to execute 'rpmbuild'")?;

  println!("{}", String::from_utf8_lossy(&output.stdout));
  eprintln!("{}", String::from_utf8_lossy(&output.stderr));

  if !output.status.success() {
    anyhow::bail!("'rpmbuild' command failed.");
  }

  log::info!("'rpmbuild' executed successfully.");
  Ok(())
}

// collect_artifacts now returns a list of found RPMs
fn collect_artifacts(rpmbuild_dir: &Path) -> Result<Vec<PathBuf>> {
  log::info!("Collecting build artifacts...");

  let rpms_dir = rpmbuild_dir.join("RPMS");
  let mut found_rpms = Vec::new();

  if rpms_dir.exists() {
    // Walk the directory to find any .rpm files
    for entry in walkdir::WalkDir::new(rpms_dir) {
      let entry = entry.context("Failed to read directory entry")?;
      if entry.path().extension().map_or(false, |e| e == "rpm") {
        log::info!("Found RPM artifact: {}", entry.path().display());
        found_rpms.push(entry.path().to_path_buf());
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
  // Add more metadata checks as needed (e.g., license)

  // 2. Verify file manifest
  if let Some(expected_assets) = &config.assets {
    log::debug!("Verifying package file manifest...");
    let actual_files: std::collections::HashSet<_> =
      metadata.get_file_paths()?.into_iter().collect();

    for asset in expected_assets {
      let expected_path = PathBuf::from(&asset.dest);
      if !actual_files.contains(&expected_path) {
        log::error!(
          "Verification failed: Expected file not found in package: {}",
          asset.dest
        );
        issues_found += 1;
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
