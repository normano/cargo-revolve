mod common;

use common::create_revolve_command;
use serial_test::serial;
use std::fs;
use std::path::Path;

const CUSTOM_FIXTURE_DIR: &str = "tests/fixtures/custom-build-project";

#[test]
#[serial]
fn test_custom_build_command_happy_path() {
  if which::which("rpmbuild").is_err() {
    println!("SKIPPING TEST: `rpmbuild` command not found in PATH.");
    return;
  }
  
  // Custom setup for this fixture
  let fixture_path = Path::new(CUSTOM_FIXTURE_DIR);
  let _ = fs::remove_dir_all(fixture_path.join("target"));
  let _ = fs::remove_dir_all(fixture_path.join("dist")); // Clean dist too
  
  // Make sure the script is executable
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    let script_path = fixture_path.join("build-script.sh");
    fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();
  }

  let mut cmd = create_revolve_command();
  // Add the --no-archive flag
  let assert = cmd
    .current_dir(CUSTOM_FIXTURE_DIR)
    .arg("build")
    .arg("--no-archive") // This is the crucial change
    .assert()
    .success();
    
  let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

  // Assert that our script's output was streamed
  assert!(output.contains("Custom build script is running!"));
  assert!(output.contains("Artifact created successfully."));

  // Assert that the artifact was actually created
  let artifact_path = fixture_path.join("target/custom-artifact.txt");
  assert!(artifact_path.exists(), "Custom build script did not create the expected artifact");
  
  // Assert that the final RPM was built
  let rpm_path = fixture_path.join("target/revolve/rpmbuild/RPMS");
  let rpm_files: Vec<_> = walkdir::WalkDir::new(&rpm_path)
    .into_iter()
    .filter_map(|e| e.ok())
    .filter(|e| e.path().extension().map_or(false, |ext| ext == "rpm"))
    .collect();
  assert!(!rpm_files.is_empty(), "Expected an RPM to be built after custom command");
}