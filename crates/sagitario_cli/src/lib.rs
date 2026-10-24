//! Command-line interface of the `sag` compiler: argument parsing and command dispatch.

use std::fmt;
use std::path::PathBuf;
use std::process::ExitCode;

pub const USAGE: &str = "\
Usage: sag <command> [options] <file.sag>

Commands:
  build    Compile a program to an executable
  run      Compile and run a program (arguments after `--` go to the program)
  check    Check a program without generating code
  help     Show this message
  version  Show the compiler version

Options:
  --release                   Optimize; integer overflow wraps instead of panicking (build, run)
  --sanitize                  Detect memory errors and undefined behavior with ASan and UBSan,
                              where the C compiler supports them (build, run)
  --emit-c                    Also write the generated C (build, run)
  --cc <compiler>             C compiler to use; overrides SAG_CC (build, run)
  --diagnostic-format <fmt>   `human` (default) or `short`: one line per diagnostic";

/// Exit code for invalid command-line usage.
pub const EXIT_USAGE: u8 = 2;

pub use sagitario_diagnostics::DiagnosticFormat;
pub use sagitario_driver::CompileOptions;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
  Build(CompileOptions),
  Run(CompileOptions),
  Check(CompileOptions),
  Help,
  Version,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageError(String);

impl fmt::Display for UsageError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.0)
  }
}

impl std::error::Error for UsageError {}

fn usage_error(message: impl Into<String>) -> UsageError {
  UsageError(message.into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
  Build,
  Run,
  Check,
}

/// Parses the arguments that follow the program name.
pub fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Result<Command, UsageError> {
  let mut args = args.into_iter();
  let Some(command) = args.next() else {
    return Err(usage_error("missing command"));
  };

  match command.as_str() {
    "build" => parse_compile_options(args, Mode::Build).map(Command::Build),
    "run" => parse_compile_options(args, Mode::Run).map(Command::Run),
    "check" => parse_compile_options(args, Mode::Check).map(Command::Check),
    "help" | "--help" | "-h" => Ok(Command::Help),
    "version" | "--version" | "-v" => Ok(Command::Version),
    other => Err(usage_error(format!("unknown command `{other}`"))),
  }
}

fn parse_compile_options<I: Iterator<Item = String>>(mut args: I, mode: Mode) -> Result<CompileOptions, UsageError> {
  let mut options = CompileOptions::default();
  let mut input = None;

  while let Some(arg) = args.next() {
    let (flag, inline_value) = match arg.split_once('=') {
      Some((flag, value)) if flag.starts_with("--") => (flag.to_string(), Some(value.to_string())),
      _ => (arg.clone(), None),
    };

    match flag.as_str() {
      "--emit-c" => {
        require_codegen(mode, &flag)?;
        options.emit_c = true;
      }
      "--release" => {
        require_codegen(mode, &flag)?;
        options.release = true;
      }
      "--sanitize" => {
        require_codegen(mode, &flag)?;
        options.sanitize = true;
      }
      "--cc" => {
        require_codegen(mode, &flag)?;
        options.cc = Some(flag_value(&flag, inline_value, &mut args)?);
      }
      "--diagnostic-format" => {
        options.diagnostic_format = match flag_value(&flag, inline_value, &mut args)?.as_str() {
          "human" => DiagnosticFormat::Human,
          "short" => DiagnosticFormat::Short,
          other => return Err(usage_error(format!("unknown diagnostic format `{other}`; expected `human` or `short`"))),
        };
      }
      "--" => {
        if mode != Mode::Run {
          return Err(usage_error("program arguments after `--` are only accepted by `sag run`"));
        }
        options.program_args = args.by_ref().collect();
      }
      _ if flag.starts_with('-') => return Err(usage_error(format!("unknown option `{flag}`"))),
      _ => {
        if input.is_some() {
          return Err(usage_error(format!("unexpected argument `{arg}`; only one input file is accepted")));
        }
        input = Some(PathBuf::from(arg));
      }
    }
  }
  options.input = input.ok_or_else(|| usage_error("missing input file"))?;
  Ok(options)
}

fn require_codegen(mode: Mode, flag: &str) -> Result<(), UsageError> {
  if mode == Mode::Check {
    return Err(usage_error(format!("`{flag}` is not accepted by `sag check`, which generates no code")));
  }
  Ok(())
}

fn flag_value<I: Iterator<Item = String>>(flag: &str, inline_value: Option<String>, args: &mut I) -> Result<String, UsageError> {
  inline_value.or_else(|| args.next()).ok_or_else(|| usage_error(format!("`{flag}` requires a value")))
}

/// Runs a parsed command and returns the process exit code.
pub fn execute(command: &Command) -> ExitCode {
  match command {
    Command::Help => {
      println!("{USAGE}");
      ExitCode::SUCCESS
    }
    Command::Version => {
      println!("sag {}", env!("CARGO_PKG_VERSION"));
      ExitCode::SUCCESS
    }
    Command::Build(options) => ExitCode::from(sagitario_driver::build(options)),
    Command::Run(options) => ExitCode::from(sagitario_driver::run(options)),
    Command::Check(options) => ExitCode::from(sagitario_driver::check(options)),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn parse(args: &[&str]) -> Result<Command, UsageError> {
    parse_args(args.iter().map(|arg| arg.to_string()))
  }

  fn options(input: &str) -> CompileOptions {
    CompileOptions { input: PathBuf::from(input), ..CompileOptions::default() }
  }

  #[test]
  fn parses_each_compile_command() {
    assert_eq!(parse(&["build", "main.sag"]), Ok(Command::Build(options("main.sag"))));
    assert_eq!(parse(&["run", "main.sag"]), Ok(Command::Run(options("main.sag"))));
    assert_eq!(parse(&["check", "main.sag"]), Ok(Command::Check(options("main.sag"))));
  }

  #[test]
  fn parses_help_and_version_aliases() {
    for alias in ["help", "--help", "-h"] {
      assert_eq!(parse(&[alias]), Ok(Command::Help));
    }
    for alias in ["version", "--version", "-v"] {
      assert_eq!(parse(&[alias]), Ok(Command::Version));
    }
  }

  #[test]
  fn parses_codegen_flags_in_both_forms() {
    let expected = CompileOptions { emit_c: true, cc: Some("gcc".to_string()), ..options("main.sag") };
    assert_eq!(parse(&["build", "--emit-c", "--cc", "gcc", "main.sag"]), Ok(Command::Build(expected.clone())));
    assert_eq!(parse(&["build", "main.sag", "--cc=gcc", "--emit-c"]), Ok(Command::Build(expected)));
  }

  #[test]
  fn parses_diagnostic_format() {
    let expected = CompileOptions { diagnostic_format: DiagnosticFormat::Short, ..options("main.sag") };
    assert_eq!(parse(&["check", "--diagnostic-format", "short", "main.sag"]), Ok(Command::Check(expected)));
    assert!(parse(&["check", "--diagnostic-format=json", "main.sag"]).is_err());
  }

  #[test]
  fn forwards_program_arguments_only_for_run() {
    let expected = CompileOptions { program_args: vec!["--verbose".to_string(), "x".to_string()], ..options("main.sag") };
    assert_eq!(parse(&["run", "main.sag", "--", "--verbose", "x"]), Ok(Command::Run(expected)));
    assert!(parse(&["build", "main.sag", "--", "x"]).is_err());
  }

  #[test]
  fn parses_sanitize() {
    let expected = CompileOptions { sanitize: true, ..options("main.sag") };

    assert_eq!(parse(&["run", "--sanitize", "main.sag"]), Ok(Command::Run(expected)));
    assert!(parse(&["check", "--sanitize", "main.sag"]).is_err());
  }

  #[test]
  fn parses_release() {
    let expected = CompileOptions { release: true, ..options("main.sag") };

    assert_eq!(parse(&["run", "--release", "main.sag"]), Ok(Command::Run(expected)));
  }

  #[test]
  fn rejects_codegen_flags_for_check() {
    assert!(parse(&["check", "--release", "main.sag"]).is_err());
    assert!(parse(&["check", "--emit-c", "main.sag"]).is_err());
    assert!(parse(&["check", "--cc", "clang", "main.sag"]).is_err());
  }

  #[test]
  fn rejects_invalid_usage() {
    assert!(parse(&[]).is_err());
    assert!(parse(&["compile", "main.sag"]).is_err());
    assert!(parse(&["build"]).is_err());
    assert!(parse(&["build", "a.sag", "b.sag"]).is_err());
    assert!(parse(&["build", "--unknown", "main.sag"]).is_err());
    assert!(parse(&["build", "main.sag", "--cc"]).is_err());
  }
}
