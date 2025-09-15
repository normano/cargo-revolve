use assert_cmd::Command;

/// Helper to create the command correctly, simulating `cargo revolve ...`.
/// This is the key fix for the original panic.
pub fn create_revolve_command() -> Command {
  let mut cmd = Command::cargo_bin("cargo-revolve").unwrap();
  cmd.arg("revolve");
  cmd
}