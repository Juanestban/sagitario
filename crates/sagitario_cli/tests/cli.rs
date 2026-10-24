//! Behavior of the `sag` binary: exit codes and messages.

use std::io;
use std::process::{Command, Output};

fn sag(args: &[&str]) -> io::Result<Output> {
  Command::new(env!("CARGO_BIN_EXE_sag")).args(args).output()
}

#[test]
fn version_prints_the_package_version() {
  let output = sag(&["--version"]).unwrap();

  assert!(output.status.success());
  assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), format!("sag {}", env!("CARGO_PKG_VERSION")));
}

#[test]
fn help_succeeds() {
  let output = sag(&["help"]).unwrap();

  assert!(output.status.success());
  assert!(String::from_utf8_lossy(&output.stdout).contains("Usage: sag"));
}

#[test]
fn invalid_usage_exits_with_2() {
  for args in [&[][..], &["compile"][..], &["build"][..]] {
    let output = sag(args).unwrap();

    assert_eq!(output.status.code(), Some(2), "args: {args:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("Usage: sag"));
  }
}

#[test]
fn missing_input_exits_with_1() {
  let output = sag(&["check", "does-not-exist.sag"]).unwrap();

  assert_eq!(output.status.code(), Some(1));
  assert!(String::from_utf8_lossy(&output.stderr).contains("cannot find"));
}

#[test]
fn non_sag_input_exits_with_1() {
  let output = sag(&["check", "Cargo.toml"]).unwrap();

  assert_eq!(output.status.code(), Some(1));
  assert!(String::from_utf8_lossy(&output.stderr).contains("is not a `.sag` file"));
}

#[test]
fn targets_lists_the_cross_compilation_targets() {
  let output = sag(&["targets"]).unwrap();

  assert!(output.status.success());

  let text = String::from_utf8_lossy(&output.stdout);

  for target in ["aarch64-linux-android", "armv7-linux-androideabi", "x86_64-linux-android", "aarch64-apple-ios", "aarch64-apple-ios-sim"] {
    assert!(text.contains(target), "{text}");
  }
}
