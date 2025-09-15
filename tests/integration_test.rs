mod common;

use common::create_revolve_command;
use rpm::{FileEntry, Package};
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
  // 1. PRE-CHECK: Ensure `rpmbuild` is available on the system.
  if which::which("rpmbuild").is_err() {
    println!("SKIPPING TEST: `rpmbuild` command not found in PATH.");
    return;
  }
  // 2. SETUP: Clean up any artifacts from previous test runs.
  setup_test();

  // 3. FIXTURE SETUP: Programmatically create the test files and directories.
  // This makes the test self-contained and guarantees the correct file structure.
  let fixture_path = Path::new(FIXTURE_DIR);
  let config_dir = fixture_path.join("config");
  let nested_dir = config_dir.join("nested"); // Define the nested directory path
  fs::create_dir_all(&nested_dir).unwrap();
  fs::write(config_dir.join("app.toml"), "port = 8080").unwrap();
  fs::write(nested_dir.join("extra.toml"), "enabled = true").unwrap(); // Create the nested file
  // Create the dummy service file that corresponds to our new asset.
  fs::write(
    fixture_path.join("sample.service"),
    "[Unit]\nDescription=Test\n",
  )
  .unwrap();

  // 4. EXECUTION: Run the `cargo revolve build` command.
  let mut cmd = create_revolve_command();
  cmd.current_dir(FIXTURE_DIR).arg("build").assert().success();

  // 5. ARTIFACT DISCOVERY: Find the generated RPM file in the configured output directory.
  let output_dir = Path::new(FIXTURE_DIR).join("dist");
  assert!(
    output_dir.exists(),
    "RPM output directory 'dist' was not created"
  );

  // Find the specific binary RPM file we want to inspect.
  let rpm_path = walkdir::WalkDir::new(&output_dir)
    .into_iter()
    .filter_map(|e| e.ok())
    .find(|e| {
      let filename = e.file_name().to_string_lossy();
      // Ensure we get the main binary RPM and not a debug or source package.
      filename.starts_with("sample-project-0.1.0")
        && filename.ends_with(".rpm")
        && !filename.contains("debuginfo")
        && !filename.contains("debugsource")
    })
    .map(|e| e.into_path())
    .expect("Expected binary RPM file was not found in the 'dist' directory");

  // 6. INSPECTION: Open the generated RPM package to analyze its contents.
  let package = Package::open(&rpm_path)
    .unwrap_or_else(|_| panic!("Failed to open and parse RPM at {}", rpm_path.display()));

  // Get a list of all file entries, which includes files and directories along with their metadata.
  let file_entries: Vec<FileEntry> = package.metadata.get_file_entries().unwrap();

  // Create a helper closure to make finding specific entries by their path easy.
  let find_entry = |path: &str| -> Option<&FileEntry> {
    file_entries.iter().find(|e| e.path == PathBuf::from(path))
  };

  // 7. VERIFICATION & ASSERTIONS:

  // A. Verify the main binary file is present.
  assert!(
    find_entry("/usr/bin/sample-project").is_some(),
    "RPM is missing the binary file"
  );

  // B. Verify that all files from the expanded directory are present.
  let expected_config_file = "/etc/sample-project/conf.d/app.toml";
  let expected_nested_file = "/etc/sample-project/conf.d/nested/extra.toml";

  assert!(
    find_entry(expected_config_file).is_some(),
    "RPM is missing the expanded config file: {}",
    expected_config_file
  );
  assert!(
    find_entry(expected_nested_file).is_some(),
    "RPM is missing the expanded nested config file: {}",
    expected_nested_file
  );

  // C. Verify that the directories themselves were explicitly created and are owned by the package.
  let top_level_dir_entry = find_entry("/etc/sample-project/conf.d")
    .expect("RPM is missing the top-level directory '/etc/sample-project/conf.d'");

  let nested_dir_entry = find_entry("/etc/sample-project/conf.d/nested")
    .expect("RPM is missing the nested directory '/etc/sample-project/conf.d/nested'");

  // D. Verify the entry types and ownership using the correct field names.
  // The is_dir() helper method needs to be implemented on the FileMode enum.
  // Based on your provided source, we can write our own helper for the test.
  let is_dir = |mode: &rpm::FileMode| matches!(mode, rpm::FileMode::Dir { .. });

  assert!(
    is_dir(&top_level_dir_entry.mode),
    "Expected '/etc/sample-project/conf.d' to be a directory entry"
  );
  assert!(
    is_dir(&nested_dir_entry.mode),
    "Expected '/etc/sample-project/conf.d/nested' to be a directory entry"
  );

  // Since the fixture's spec uses the default `%defattr(-, root, root, -)`, we verify that ownership is correct.
  assert_eq!(
    top_level_dir_entry.ownership.user, "root",
    "Top-level directory user ownership is incorrect"
  );
  assert_eq!(
    top_level_dir_entry.ownership.group, "root",
    "Top-level directory group ownership is incorrect"
  );
  assert_eq!(
    nested_dir_entry.ownership.user, "root",
    "Nested directory user ownership is incorrect"
  );
  assert_eq!(
    nested_dir_entry.ownership.group, "root",
    "Nested directory group ownership is incorrect"
  );

  // E. Verify the systemd service file was packaged correctly.
  let service_file_path = "/usr/lib/systemd/system/sample.service";
  assert!(
    find_entry(service_file_path).is_some(),
    "RPM is missing the service file: {}",
    service_file_path
  );

  // F. CRITICAL: Verify that the package did NOT take ownership of the system directory.
  let systemd_dir_path = "/usr/lib/systemd/system";
  assert!(
    find_entry(systemd_dir_path).is_none(),
    "DANGER: RPM has taken ownership of the system directory '{}'",
    systemd_dir_path
  );

  println!(
    "Successfully verified RPM contents, including expanded directories and their ownership."
  );
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
