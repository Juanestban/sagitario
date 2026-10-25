//! Cross-compilation targets (docs/nova.md#targets): Android through the NDK, and iOS through
//! Xcode on macOS.

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use sagitario_ast::TargetInfo;

use crate::{CCompiler, EXIT_FAILURE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
  Android,
  Ios,
}

/// A machine programs can be compiled for, besides the one running the compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
  /// The name `--target` takes, as in Rust.
  pub name: &'static str,
  /// What `sag targets` says about it.
  pub description: &'static str,
  pub platform: Platform,
  /// The start of clang's triple.
  arch: &'static str,
  /// An iOS simulator rather than a device.
  simulator: bool,
  pub info: TargetInfo,
}

/// The minimum Android API level: Android 7.0.
const ANDROID_API: u32 = 24;

/// The minimum iOS version.
const IOS_VERSION: &str = "13.0";

const LP64: TargetInfo = TargetInfo { pointer_width: 64, long_is_32: false, char_is_unsigned: false };

pub const TARGETS: [Target; 6] = [
  Target {
    name: "aarch64-linux-android",
    description: "Android on 64-bit ARM",
    platform: Platform::Android,
    arch: "aarch64-linux-android",
    simulator: false,
    info: TargetInfo { char_is_unsigned: true, ..LP64 },
  },
  Target {
    name: "armv7-linux-androideabi",
    description: "Android on 32-bit ARM",
    platform: Platform::Android,
    arch: "armv7a-linux-androideabi",
    simulator: false,
    info: TargetInfo { pointer_width: 32, long_is_32: true, char_is_unsigned: true },
  },
  Target {
    name: "x86_64-linux-android",
    description: "Android on x86-64 (the emulator)",
    platform: Platform::Android,
    arch: "x86_64-linux-android",
    simulator: false,
    info: LP64,
  },
  Target { name: "aarch64-apple-ios", description: "iOS devices", platform: Platform::Ios, arch: "arm64-apple-ios", simulator: false, info: LP64 },
  Target {
    name: "aarch64-apple-ios-sim",
    description: "the iOS simulator on Apple silicon",
    platform: Platform::Ios,
    arch: "arm64-apple-ios",
    simulator: true,
    info: LP64,
  },
  Target {
    name: "x86_64-apple-ios",
    description: "the iOS simulator on Intel Macs",
    platform: Platform::Ios,
    arch: "x86_64-apple-ios",
    simulator: true,
    info: LP64,
  },
];

/// The target called `name`, or an error listing the supported ones.
pub fn find(name: &str) -> Result<&'static Target, u8> {
  if let Some(target) = TARGETS.iter().find(|target| target.name == name) {
    return Ok(target);
  }

  let names: Vec<&str> = TARGETS.iter().map(|target| target.name).collect();

  eprintln!("error: unknown target `{name}`");

  if let Some(similar) = sagitario_diagnostics::closest_match(name, names.iter().copied()) {
    eprintln!("help: a target with a similar name exists: `{similar}`");
  } else {
    eprintln!("note: the supported targets are {}; `sag targets` describes them", names.join(", "));
  }

  Err(EXIT_FAILURE)
}

impl Target {
  /// The triple clang compiles for.
  fn clang_triple(&self) -> String {
    match self.platform {
      Platform::Android => format!("{}{ANDROID_API}", self.arch),
      Platform::Ios if self.simulator => format!("{}{IOS_VERSION}-simulator", self.arch),
      Platform::Ios => format!("{}{IOS_VERSION}", self.arch),
    }
  }

  /// The C compiler for this target, with the arguments every invocation needs.
  pub(crate) fn compiler(&self) -> Result<CCompiler, u8> {
    let (command, mut args) = match self.platform {
      Platform::Android => (android_clang()?, Vec::new()),
      Platform::Ios => ios_clang(self.simulator)?,
    };

    args.insert(0, format!("--target={}", self.clang_triple()));

    let output = Command::new(&command).args(&args).arg("-dumpmachine").stdin(Stdio::null()).output();

    match output {
      Ok(output) if output.status.success() => {
        let target = String::from_utf8_lossy(&output.stdout).trim().to_string();

        Ok(CCompiler { command, args, target, sanitized: false, cl: false })
      }
      _ => {
        eprintln!("error: cannot run the C compiler `{command}` for the target `{}`", self.name);
        Err(EXIT_FAILURE)
      }
    }
  }
}

/// The NDK's directory: `ANDROID_NDK_HOME`, `ANDROID_NDK_ROOT`, then the newest NDK in the `ndk`
/// directory of `ANDROID_HOME` or `ANDROID_SDK_ROOT`.
fn find_ndk() -> Option<PathBuf> {
  let is_ndk = |dir: &Path| dir.join("toolchains").join("llvm").join("prebuilt").is_dir();
  let from_env = |name: &str| env::var_os(name).map(PathBuf::from).filter(|dir| !dir.as_os_str().is_empty());

  for name in ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT"] {
    if let Some(dir) = from_env(name).filter(|dir| is_ndk(dir)) {
      return Some(dir);
    }
  }

  for name in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
    let Some(sdk) = from_env(name) else { continue };
    let Ok(entries) = std::fs::read_dir(sdk.join("ndk")) else { continue };
    let mut versions: Vec<(Vec<u64>, PathBuf)> = entries
      .flatten()
      .map(|entry| entry.path())
      .filter(|dir| is_ndk(dir))
      .map(|dir| {
        let version =
          dir.file_name().map(|name| name.to_string_lossy().split('.').map(|part| part.parse().unwrap_or(0)).collect()).unwrap_or_default();

        (version, dir)
      })
      .collect();

    versions.sort();

    if let Some((_, dir)) = versions.pop() {
      return Some(dir);
    }
  }

  None
}

/// The NDK's clang, in the one host directory of `toolchains/llvm/prebuilt`.
fn android_clang() -> Result<String, u8> {
  let Some(ndk) = find_ndk() else {
    eprintln!("error: cannot find the Android NDK");
    eprintln!(
      "help: install it with Android Studio's SDK Manager, then set ANDROID_NDK_HOME to its directory (such as `.../Android/Sdk/ndk/26.1.10909125`)"
    );
    return Err(EXIT_FAILURE);
  };
  let prebuilt = ndk.join("toolchains").join("llvm").join("prebuilt");
  let host = std::fs::read_dir(&prebuilt).ok().and_then(|entries| entries.flatten().map(|entry| entry.path()).find(|dir| dir.is_dir()));
  let clang = host.map(|dir| dir.join("bin").join("clang").with_extension(env::consts::EXE_EXTENSION));

  match clang.filter(|clang| clang.is_file()) {
    Some(clang) => Ok(clang.display().to_string()),
    None => {
      eprintln!("error: the Android NDK at `{}` has no clang in `toolchains/llvm/prebuilt/*/bin`", ndk.display());
      Err(EXIT_FAILURE)
    }
  }
}

/// Xcode's clang and the arguments selecting the iOS SDK, through `xcrun`.
fn ios_clang(simulator: bool) -> Result<(String, Vec<String>), u8> {
  if !cfg!(target_os = "macos") {
    eprintln!("error: code for iOS can only be built on macOS, with Xcode");
    return Err(EXIT_FAILURE);
  }

  let sdk = if simulator { "iphonesimulator" } else { "iphoneos" };
  let xcrun = |query: &[&str]| {
    let output = Command::new("xcrun").args(["--sdk", sdk]).args(query).stdin(Stdio::null()).output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();

    (output.status.success() && !text.is_empty()).then_some(text)
  };
  let (Some(clang), Some(sysroot)) = (xcrun(&["--find", "clang"]), xcrun(&["--show-sdk-path"])) else {
    eprintln!("error: cannot find the iOS SDK (`{sdk}`) with `xcrun`");
    eprintln!("help: install Xcode, then run `xcode-select --install` and open Xcode once");
    return Err(EXIT_FAILURE);
  };

  Ok((clang, vec!["-isysroot".to_string(), sysroot]))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn builds_clang_triples() {
    let triples: Vec<String> = TARGETS.iter().map(Target::clang_triple).collect();

    assert_eq!(
      triples,
      vec![
        "aarch64-linux-android24",
        "armv7a-linux-androideabi24",
        "x86_64-linux-android24",
        "arm64-apple-ios13.0",
        "arm64-apple-ios13.0-simulator",
        "x86_64-apple-ios13.0-simulator"
      ]
    );
  }

  #[test]
  fn finds_targets_by_name() {
    assert!(find("aarch64-linux-android").is_ok());
    assert!(find("aarch64-linux-androi").is_err());
  }
}
