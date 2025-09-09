use assert_cmd::Command;
use serial_test::serial;
use std::fs;
use std::path::Path;

const FIXTURE_DIR: &str = "tests/fixtures/sample-project";

/// Helper to set up the test environment.
fn setup_test() {
  // Clean up previous test runs to ensure a clean slate.
  let _ = fs::remove_dir_all(Path::new(FIXTURE_DIR).join("target"));
}

/// Helper to create the command correctly, simulating `cargo revolve ...`.
/// This is the key fix for the original panic.
fn create_revolve_command() -> Command {
  let mut cmd = Command::cargo_bin("cargo-revolve").unwrap();
  cmd.arg("revolve");
  cmd
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
