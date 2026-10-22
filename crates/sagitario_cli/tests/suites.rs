//! End-to-end harness: runs the `.sag` suites under `tests/` through the `sag` binary.
//! The annotation format is documented in `docs/testing.md`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Suite {
  RunPass,
  CompileFail,
  RunPanic,
}

impl Suite {
  const ALL: [Suite; 3] = [Suite::RunPass, Suite::CompileFail, Suite::RunPanic];

  fn dir_name(self) -> &'static str {
    match self {
      Suite::RunPass => "run-pass",
      Suite::CompileFail => "compile-fail",
      Suite::RunPanic => "run-panic",
    }
  }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Expectations {
  stdout: Vec<String>,
  exit: Option<i32>,
  panic: Option<String>,
  errors: Vec<ExpectedError>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ExpectedError {
  line: usize,
  code: String,
}

fn parse_expectations(source: &str) -> Result<Expectations, String> {
  let mut expectations = Expectations::default();
  for (index, line) in source.lines().enumerate() {
    let line_number = index + 1;
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix("// expect-stdout:") {
      expectations.stdout.push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
    } else if let Some(rest) = trimmed.strip_prefix("// expect-exit:") {
      let value = rest.trim();
      let code = value.parse().map_err(|_| format!("line {line_number}: invalid `expect-exit` value `{value}`"))?;
      expectations.exit = Some(code);
    } else if let Some(rest) = trimmed.strip_prefix("// expect-panic:") {
      expectations.panic = Some(rest.trim().to_string());
    }
    if let Some((_, annotation)) = line.split_once("//~") {
      let annotation = annotation.trim();
      let code = annotation
        .strip_prefix("ERROR")
        .map(str::trim)
        .filter(|code| is_diagnostic_code(code))
        .ok_or_else(|| format!("line {line_number}: malformed annotation `//~ {annotation}`; expected `//~ ERROR Ennnn`"))?;
      expectations.errors.push(ExpectedError { line: line_number, code: code.to_string() });
    }
  }
  Ok(expectations)
}

fn is_diagnostic_code(code: &str) -> bool {
  code.strip_prefix('E').is_some_and(|digits| digits.len() == 4 && digits.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Extracts errors from `--diagnostic-format short` output: `path:line:column: error[E0001]: message`.
/// The path is split from the right because Windows paths contain `:`.
fn parse_reported_errors(stderr: &str) -> Vec<ExpectedError> {
  stderr
    .lines()
    .filter_map(|line| {
      let (location, rest) = line.split_once(": error[")?;
      let (code, _) = rest.split_once(']')?;
      let mut parts = location.rsplitn(3, ':');
      let _column = parts.next()?;
      let line_number = parts.next()?.parse().ok()?;
      Some(ExpectedError { line: line_number, code: code.to_string() })
    })
    .collect()
}

fn workspace_root() -> Result<PathBuf, String> {
  Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).map(Path::to_path_buf).ok_or_else(|| "cannot locate the workspace root".to_string())
}

fn discover(root: &Path, suite: Suite) -> Result<Vec<PathBuf>, String> {
  let dir = root.join("tests").join(suite.dir_name());
  let entries = fs::read_dir(&dir).map_err(|error| format!("cannot read suite directory `{}`: {error}", dir.display()))?;
  let mut cases = Vec::new();
  for entry in entries {
    let path = entry.map_err(|error| format!("cannot read an entry of `{}`: {error}", dir.display()))?.path();
    if path.extension().is_some_and(|extension| extension == "sag") {
      cases.push(path);
    }
  }
  cases.sort();
  Ok(cases)
}

fn sag(args: &[&str], file: &Path) -> Result<Output, String> {
  Command::new(env!("CARGO_BIN_EXE_sag")).args(args).arg(file).output().map_err(|error| format!("cannot run `sag`: {error}"))
}

fn exit_code(output: &Output) -> Result<i32, String> {
  output.status.code().ok_or_else(|| "process was terminated by a signal".to_string())
}

fn expect_exit(output: &Output, expected: i32) -> Result<(), String> {
  let actual = exit_code(output)?;
  if actual == expected {
    return Ok(());
  }
  Err(format!("expected exit code {expected}, got {actual}\nstderr:\n{}", String::from_utf8_lossy(&output.stderr)))
}

fn run_case(suite: Suite, file: &Path) -> Result<(), String> {
  let source = fs::read_to_string(file).map_err(|error| format!("cannot read file: {error}"))?;
  let expectations = parse_expectations(&source)?;
  match suite {
    Suite::RunPass => {
      if !expectations.errors.is_empty() || expectations.panic.is_some() {
        return Err("run-pass tests cannot use `//~ ERROR` or `expect-panic`".to_string());
      }
      let output = sag(&["run"], file)?;
      expect_exit(&output, expectations.exit.unwrap_or(0))?;
      if !expectations.stdout.is_empty() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let actual: Vec<&str> = stdout.lines().collect();
        if actual != expectations.stdout {
          return Err(format!("stdout mismatch\nexpected: {:?}\nactual:   {actual:?}", expectations.stdout));
        }
      }
      Ok(())
    }
    Suite::RunPanic => {
      let Some(message) = &expectations.panic else {
        return Err("run-panic tests need an `expect-panic` annotation".to_string());
      };
      let output = sag(&["run"], file)?;
      expect_exit(&output, 101)?;
      let stderr = String::from_utf8_lossy(&output.stderr);
      if !stderr.contains(message.as_str()) {
        return Err(format!("panic message `{message}` not found in stderr:\n{stderr}"));
      }
      Ok(())
    }
    Suite::CompileFail => {
      if expectations.errors.is_empty() {
        return Err("compile-fail tests need at least one `//~ ERROR Ennnn` annotation".to_string());
      }
      let output = sag(&["check", "--diagnostic-format", "short"], file)?;
      expect_exit(&output, 1)?;
      let mut expected = expectations.errors;
      let mut reported = parse_reported_errors(&String::from_utf8_lossy(&output.stderr));
      expected.sort();
      reported.sort();
      if reported != expected {
        return Err(format!("diagnostics mismatch\nexpected: {expected:?}\nreported: {reported:?}"));
      }
      Ok(())
    }
  }
}

#[test]
fn sag_suites() {
  let root = workspace_root().unwrap();
  let mut total = 0;
  let mut failures = Vec::new();
  for suite in Suite::ALL {
    match discover(&root, suite) {
      Ok(cases) => {
        for case in cases {
          total += 1;
          if let Err(error) = run_case(suite, &case) {
            failures.push(format!("{}: {error}", case.display()));
          }
        }
      }
      Err(error) => failures.push(error),
    }
  }
  println!("ran {total} .sag test(s)");
  assert!(failures.is_empty(), "{} of {total} .sag test(s) failed:\n\n{}", failures.len(), failures.join("\n\n"));
}

#[test]
fn parses_all_annotation_kinds() {
  let source = "\
// expect-stdout: total = 6
// expect-stdout:  indented
// expect-exit: 3
// expect-panic: index out of bounds
const x: i32 = \"hello\";   //~ ERROR E0042
";
  let expectations = parse_expectations(source).unwrap();
  assert_eq!(expectations.stdout, vec!["total = 6".to_string(), " indented".to_string()]);
  assert_eq!(expectations.exit, Some(3));
  assert_eq!(expectations.panic.as_deref(), Some("index out of bounds"));
  assert_eq!(expectations.errors, vec![ExpectedError { line: 5, code: "E0042".to_string() }]);
}

#[test]
fn rejects_malformed_annotations() {
  assert!(parse_expectations("x; //~ ERROR 42").is_err());
  assert!(parse_expectations("x; //~ WARN E0042").is_err());
  assert!(parse_expectations("// expect-exit: zero").is_err());
}

#[test]
fn parses_short_diagnostics_including_windows_paths() {
  let stderr = "\
C:\\work\\main.sag:12:5: error[E0042]: mismatched types
tests/a.sag:3:1: error[E0001]: unexpected token
warning: something unrelated
";
  assert_eq!(
    parse_reported_errors(stderr),
    vec![ExpectedError { line: 12, code: "E0042".to_string() }, ExpectedError { line: 3, code: "E0001".to_string() }]
  );
}
