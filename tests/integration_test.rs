mod common;

use common::create_revolve_command;
use rpm::Package;
use serial_test::serial;
use std::fs;
use std::path::{Path, PathBuf};

const FIXTURE_DIR: &str = "tests/fixtures/sample-project";

/// Helper to set up the test environment.
fn setup_test() {
  // Clean up previous test runs to ensure a clean slate.
  let fixture_path = Path::new(FIXTURE_DIR);
  let _ = fs::remove_dir_all(fixture_path.join("target"));
  let _ = fs::remove_dir_all(fixture_path.join("dist"));
}

#[test]
#[serial]
fn test_build_happy_path() {
  if which::which("rpmbuild").is_err() {
    println!("SKIPPING TEST: `rpmbuild` command not found in PATH.");
    return;
  }
  setup_test();

  let mut cmd = create_revolve_command();
  cmd.current_dir(FIXTURE_DIR).arg("build").assert().success();

  let target_dir = Path::new(FIXTURE_DIR).join("target/revolve/rpmbuild/RPMS");
  assert!(target_dir.exists(), "RPM output directory was not created");

  let rpm_files: Vec<_> = walkdir::WalkDir::new(&target_dir)
    .into_iter()
    .filter_map(|e| e.ok())
    .filter(|e| e.path().extension().map_or(false, |ext| ext == "rpm"))
    .collect();

  // THE FIX:
  // Don't assert the total count. Instead, assert that at least one RPM was created,
  // and that the specific binary RPM we expect is present in the list.
  assert!(
    !rpm_files.is_empty(),
    "Expected at least one RPM file to be created"
  );

  let expected_binary_rpm_name = "sample-project-0.1.0-1"; // Arch can vary, so we check the prefix
  let binary_rpm_exists = rpm_files.iter().any(|entry| {
    let filename = entry.file_name().to_string_lossy();
    // We check that it's not a debuginfo or debugsource package
    filename.starts_with(expected_binary_rpm_name)
      && !filename.contains("debuginfo")
      && !filename.contains("debugsource")
  });

  assert!(
    binary_rpm_exists,
    "The expected binary RPM was not found. Found files: {:?}",
    rpm_files
  );
}

#[test]
#[serial]
fn test_build_verify_happy_path() {
  if which::which("rpmbuild").is_err() {
    println!("SKIPPING TEST: `rpmbuild` command not found in PATH.");
    return;
  }
  setup_test();

  let mut cmd = create_revolve_command();
  let assert = cmd
    .current_dir(FIXTURE_DIR)
    .arg("build")
    .arg("--verify")
    .assert()
    .success();

  let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
  assert!(output.contains("Verification successful."));
}

#[test]
#[serial]
fn test_dry_run() {
  // This test does not require rpmbuild, so no check is needed.
  setup_test();

  let mut cmd = create_revolve_command();
  let assert = cmd
    .current_dir(FIXTURE_DIR)
    .arg("build")
    .arg("--dry-run")
    .assert()
    .success();

  let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

  assert!(output.contains("--- Dry Run Activated ---"));
  assert!(output.contains("[1/2] Rendered .spec file"));
  assert!(output.contains("Name:           sample-project"));
  assert!(output.contains("[2/2] The following `rpmbuild` command would be executed:"));
  assert!(output.contains("rpmbuild -ta"));
}

#[test]
#[serial]
fn test_build_expands_directory_assets_and_copies_to_output_dir() {
  if which::which("rpmbuild").is_err() {
    println!("SKIPPING TEST: `rpmbuild` command not found in PATH.");
    return;
  }
  setup_test();

  // Create the fixture directory and files for the test
  let fixture_path = Path::new(FIXTURE_DIR);
  let config_dir = fixture_path.join("config");
  fs::create_dir_all(&config_dir).unwrap();
  fs::write(config_dir.join("app.toml"), "port = 8080").unwrap();
  fs::write(config_dir.join("database.toml"), "host = \"localhost\"").unwrap();


  let mut cmd = create_revolve_command();
  cmd.current_dir(FIXTURE_DIR).arg("build").assert().success();

  let output_dir = Path::new(FIXTURE_DIR).join("dist");
  assert!(output_dir.exists(), "RPM output directory 'dist' was not created");

  // Find the generated RPM file in the output directory.
  let rpm_path = walkdir::WalkDir::new(&output_dir)
    .into_iter()
    .filter_map(|e| e.ok())
    .find(|e| {
        let filename = e.file_name().to_string_lossy();
        filename.starts_with("sample-project-0.1.0") 
            && filename.ends_with(".rpm")
            && !filename.contains("debuginfo")
            && !filename.contains("debugsource")
    })
    .map(|e| e.into_path())
    .expect("Expected binary RPM file was not found in the 'dist' directory");
    
  // --- START: NEW ASSERTIONS ---
  // Use the `rpm` crate to inspect the package contents.
  let package = Package::open(&rpm_path)
      .unwrap_or_else(|_| panic!("Failed to open and parse RPM at {}", rpm_path.display()));

  let file_paths: Vec<PathBuf> = package.metadata.get_file_paths().unwrap();

  // 1. Verify the binary is present.
  assert!(
      file_paths.contains(&PathBuf::from("/usr/bin/sample-project")),
      "RPM missing binary: /usr/bin/sample-project"
  );
  
  // 2. Verify the expanded directory contents are present.
  let expected_config1 = PathBuf::from("/etc/sample-project/conf.d/app.toml");
  let expected_config2 = PathBuf::from("/etc/sample-project/conf.d/database.toml");

  assert!(
      file_paths.contains(&expected_config1),
      "RPM missing expanded file: {}", expected_config1.display()
  );
  assert!(
      file_paths.contains(&expected_config2),
      "RPM missing expanded file: {}", expected_config2.display()
  );
  
  println!("Successfully verified RPM contents: {:?}", file_paths);
}

#[test]
#[serial]
fn test_changelog_in_dry_run() {
  // This test does not require rpmbuild.
  setup_test();

  let mut cmd = create_revolve_command();
  let assert = cmd
    .current_dir(FIXTURE_DIR)
    .arg("build")
    .arg("--dry-run")
    .assert()
    .success();

  let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

  // Check that the changelog section and its content are present in the rendered spec.
  assert!(output.contains("%changelog"));
  assert!(output.contains("Initial release of the sample project."));
  assert!(output.contains("- This is a test entry."));
}