//! End-to-end harness: runs the `.sag` suites under `tests/` through the `sag` binary.
//! A case is a `.sag` file, or a directory holding a multi-file program whose entry is
//! `main.sag`. The annotation format is documented in `docs/testing.md`.

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

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
  /// Extra `sag` options, e.g. `--release`.
  flags: Vec<String>,
  /// The program's command-line arguments.
  args: Vec<String>,
  /// Lines written to the program's stdin, each followed by a newline.
  stdin: Vec<String>,
  /// Environment variables set for the program.
  env: Vec<(String, String)>,
  /// `//~ ERROR Ennnn` and `//~ WARN Wnnnn` annotations.
  diagnostics: Vec<ExpectedDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ExpectedDiagnostic {
  /// The file, relative to the case directory and with `/` separators; empty when parsing a
  /// single source text.
  file: String,
  line: usize,
  code: String,
}

/// A test case: the file passed to `sag` and the source files whose annotations count.
struct Case {
  entry: PathBuf,
  /// The directory that file names in diagnostics are relative to.
  dir: PathBuf,
  files: Vec<PathBuf>,
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
    } else if let Some(rest) = trimmed.strip_prefix("// sag-flags:") {
      expectations.flags.extend(rest.split_whitespace().map(str::to_string));
    } else if let Some(rest) = trimmed.strip_prefix("// args:") {
      expectations.args.extend(rest.split_whitespace().map(str::to_string));
    } else if let Some(rest) = trimmed.strip_prefix("// stdin:") {
      expectations.stdin.push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
    } else if let Some(rest) = trimmed.strip_prefix("// env:") {
      let (name, value) = rest.trim().split_once('=').ok_or_else(|| format!("line {line_number}: `env` needs `NAME=value`"))?;
      expectations.env.push((name.to_string(), value.to_string()));
    }

    if let Some((_, annotation)) = line.split_once("//~") {
      expectations.diagnostics.extend(parse_annotation(annotation, line_number)?);
    }
  }

  Ok(expectations)
}

/// Parses `ERROR E0300` / `WARN W0001` pairs, optionally separated by commas:
/// `//~ ERROR E0311, WARN W0001`.
fn parse_annotation(annotation: &str, line: usize) -> Result<Vec<ExpectedDiagnostic>, String> {
  let malformed = || format!("line {line}: malformed annotation `//~{annotation}`; expected `//~ ERROR Ennnn` or `//~ WARN Wnnnn`");
  let words: Vec<&str> = annotation.split([' ', ',']).filter(|word| !word.is_empty()).collect();

  if words.is_empty() || !words.len().is_multiple_of(2) {
    return Err(malformed());
  }

  words
    .chunks(2)
    .map(|pair| match pair {
      ["ERROR", code] if is_diagnostic_code(code, 'E') => Ok(ExpectedDiagnostic { file: String::new(), line, code: code.to_string() }),
      ["WARN", code] if is_diagnostic_code(code, 'W') => Ok(ExpectedDiagnostic { file: String::new(), line, code: code.to_string() }),
      _ => Err(malformed()),
    })
    .collect()
}

fn is_diagnostic_code(code: &str, prefix: char) -> bool {
  code.strip_prefix(prefix).is_some_and(|digits| digits.len() == 4 && digits.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Extracts errors and warnings from `--diagnostic-format short` output:
/// `path:line:column: error[E0001]: message`. The path is split from the right because Windows
/// paths contain `:`; it is made relative to `dir`.
fn parse_reported_diagnostics(stderr: &str, dir: &Path) -> Vec<ExpectedDiagnostic> {
  let dir = format!("{}/", normalize(&dir.display().to_string()));

  stderr
    .lines()
    .filter_map(|line| {
      let (location, rest) = line.split_once(": error[").or_else(|| line.split_once(": warning["))?;
      let (code, _) = rest.split_once(']')?;
      let mut parts = location.rsplitn(3, ':');
      let _column = parts.next()?;
      let line_number = parts.next()?.parse().ok()?;
      let path = normalize(parts.next()?);
      let file = path.strip_prefix(&dir).unwrap_or(&path).to_string();

      Some(ExpectedDiagnostic { file, line: line_number, code: code.to_string() })
    })
    .collect()
}

fn normalize(path: &str) -> String {
  path.replace('\\', "/")
}

fn workspace_root() -> Result<PathBuf, String> {
  Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).map(Path::to_path_buf).ok_or_else(|| "cannot locate the workspace root".to_string())
}

fn discover(root: &Path, suite: Suite) -> Result<Vec<Case>, String> {
  let dir = root.join("tests").join(suite.dir_name());
  let mut cases = Vec::new();

  for path in entries(&dir)? {
    if path.extension().is_some_and(|extension| extension == "sag") {
      cases.push(Case { entry: path.clone(), dir: dir.clone(), files: vec![path] });
    } else if path.join("main.sag").is_file() {
      let mut files = Vec::new();

      sources_in(&path, &mut files)?;
      cases.push(Case { entry: path.join("main.sag"), dir: path, files });
    }
  }

  Ok(cases)
}

/// The entries of a directory, sorted.
fn entries(dir: &Path) -> Result<Vec<PathBuf>, String> {
  let entries = fs::read_dir(dir).map_err(|error| format!("cannot read directory `{}`: {error}", dir.display()))?;
  let mut paths = Vec::new();

  for entry in entries {
    paths.push(entry.map_err(|error| format!("cannot read an entry of `{}`: {error}", dir.display()))?.path());
  }

  paths.sort();

  Ok(paths)
}

/// Every `.sag` file under `dir`.
fn sources_in(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
  for path in entries(dir)? {
    if path.is_dir() {
      sources_in(&path, files)?;
    } else if path.extension().is_some_and(|extension| extension == "sag") {
      files.push(path);
    }
  }

  Ok(())
}

/// The expectations of a case: those of its entry file, plus the diagnostics annotated in every
/// file of a multi-file program.
fn case_expectations(case: &Case) -> Result<Expectations, String> {
  let mut expectations = Expectations::default();

  for file in &case.files {
    let source = fs::read_to_string(file).map_err(|error| format!("cannot read `{}`: {error}", file.display()))?;
    let mut parsed = parse_expectations(&source).map_err(|error| format!("{}: {error}", file.display()))?;
    let relative = file.strip_prefix(&case.dir).map_or_else(|_| file.display().to_string(), |path| normalize(&path.display().to_string()));

    for diagnostic in &mut parsed.diagnostics {
      diagnostic.file = relative.clone();
    }

    expectations.diagnostics.append(&mut parsed.diagnostics);

    if *file == case.entry {
      expectations.stdout = parsed.stdout;
      expectations.exit = parsed.exit;
      expectations.panic = parsed.panic;
      expectations.flags = parsed.flags;
      expectations.args = parsed.args;
      expectations.stdin = parsed.stdin;
      expectations.env = parsed.env;
    }
  }

  Ok(expectations)
}

/// `sag run <flags> <file> -- <args>` with the case's stdin and environment, and with
/// sanitizers unless `SAG_TEST_SANITIZE=0` (docs/testing.md#memory-checks).
fn run(expectations: &Expectations, file: &Path) -> Result<Output, String> {
  let mut command = sag_command(&["run"]);

  if std::env::var("SAG_TEST_SANITIZE").map_or(true, |value| value != "0") {
    command.arg("--sanitize");
  }

  command.args(&expectations.flags).arg(file).arg("--").args(&expectations.args);
  command.envs(expectations.env.iter().map(|(name, value)| (name, value)));
  command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());

  let mut child = command.spawn().map_err(|error| format!("cannot run `sag`: {error}"))?;
  let input: String = expectations.stdin.iter().map(|line| format!("{line}\n")).collect();

  // Dropping stdin closes it, so the program sees the end of its input.
  if let Some(mut stdin) = child.stdin.take() {
    stdin.write_all(input.as_bytes()).map_err(|error| format!("cannot write the program's stdin: {error}"))?;
  }

  child.wait_with_output().map_err(|error| format!("cannot run `sag`: {error}"))
}

fn sag(args: &[&str], file: &Path) -> Result<Output, String> {
  sag_command(args).arg(file).output().map_err(|error| format!("cannot run `sag`: {error}"))
}

fn sag_command(args: &[&str]) -> Command {
  let mut command = Command::new(env!("CARGO_BIN_EXE_sag"));

  // Programs fail with exit code 102 if ARC objects leak (docs/testing.md#memory-checks).
  command.args(args).env("SAG_LEAK_CHECK", "1");
  command
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

fn run_case(suite: Suite, case: &Case) -> Result<(), String> {
  let expectations = case_expectations(case)?;
  let file = case.entry.as_path();

  match suite {
    Suite::RunPass => {
      if !expectations.diagnostics.is_empty() || expectations.panic.is_some() {
        return Err("run-pass tests cannot use `//~` annotations or `expect-panic`".to_string());
      }
      let output = run(&expectations, file)?;
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
      let output = run(&expectations, file)?;

      expect_exit(&output, 101)?;

      let stderr = String::from_utf8_lossy(&output.stderr);

      if !stderr.contains(message.as_str()) {
        return Err(format!("panic message `{message}` not found in stderr:\n{stderr}"));
      }

      Ok(())
    }
    Suite::CompileFail => {
      if expectations.diagnostics.is_empty() {
        return Err("compile-fail tests need at least one `//~ ERROR` or `//~ WARN` annotation".to_string());
      }

      let output = sag(&["check", "--diagnostic-format", "short"], file)?;
      // A file with only warnings compiles.
      let has_errors = expectations.diagnostics.iter().any(|diagnostic| diagnostic.code.starts_with('E'));

      expect_exit(&output, if has_errors { 1 } else { 0 })?;

      // Every reported error and warning must be annotated, and vice versa.
      let mut expected = expectations.diagnostics;
      let mut reported = parse_reported_diagnostics(&String::from_utf8_lossy(&output.stderr), &case.dir);

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
            failures.push(format!("{}: {error}", case.entry.display()));
          }
        }
      }
      Err(error) => failures.push(error),
    }
  }

  println!("ran {total} .sag test(s)");
  assert!(failures.is_empty(), "{} of {total} .sag test(s) failed:\n\n{}", failures.len(), failures.join("\n\n"));
}

/// The Definition of Done asks for a `compile-fail` test per rule; this keeps every registered
/// diagnostic code covered.
#[test]
fn every_diagnostic_code_has_a_compile_fail_test() {
  let root = workspace_root().unwrap();
  let mut covered = HashSet::new();

  for case in discover(&root, Suite::CompileFail).unwrap() {
    covered.extend(case_expectations(&case).unwrap().diagnostics.into_iter().map(|diagnostic| diagnostic.code));
  }

  let missing: Vec<&str> = sagitario_diagnostics::codes::ALL.iter().map(|code| code.0).filter(|code| !covered.contains(*code)).collect();

  assert!(missing.is_empty(), "diagnostic codes without a compile-fail test: {missing:?}");
}

#[test]
fn parses_all_annotation_kinds() {
  let source = "\
// expect-stdout: total = 6
// expect-stdout:  indented
// expect-exit: 3
// expect-panic: index out of bounds
// args: a b
// stdin: first line
// stdin:
// env: NAME=a=b
const x: i32 = \"hello\";   //~ ERROR E0042
var y; //~ ERROR E0311, WARN W0001 WARN W0002
";
  let expectations = parse_expectations(source).unwrap();
  assert_eq!(expectations.stdout, vec!["total = 6".to_string(), " indented".to_string()]);
  assert_eq!(expectations.exit, Some(3));
  assert_eq!(expectations.panic.as_deref(), Some("index out of bounds"));
  assert_eq!(expectations.args, vec!["a".to_string(), "b".to_string()]);
  assert_eq!(expectations.stdin, vec!["first line".to_string(), String::new()]);
  assert_eq!(expectations.env, vec![("NAME".to_string(), "a=b".to_string())]);

  let codes: Vec<(usize, &str)> = expectations.diagnostics.iter().map(|diagnostic| (diagnostic.line, diagnostic.code.as_str())).collect();
  assert_eq!(codes, vec![(9, "E0042"), (10, "E0311"), (10, "W0001"), (10, "W0002")]);
}

#[test]
fn rejects_malformed_annotations() {
  assert!(parse_expectations("x; //~ ERROR 42").is_err());
  assert!(parse_expectations("x; //~ WARN E0042").is_err());
  assert!(parse_expectations("x; //~ ERROR W0001").is_err());
  assert!(parse_expectations("x; //~ ERROR").is_err());
  assert!(parse_expectations("// expect-exit: zero").is_err());
  assert!(parse_expectations("// env: NAME").is_err());
}

#[test]
fn parses_short_diagnostics_including_windows_paths() {
  let stderr = "\
C:\\work\\main.sag:12:5: error[E0042]: mismatched types
tests/a.sag:3:1: error[E0001]: unexpected token
tests/a.sag:4:7: warning[W0001]: unused binding `x`
warning: something unrelated
";
  let codes: Vec<(String, usize, String)> = parse_reported_diagnostics(stderr, Path::new("tests"))
    .into_iter()
    .map(|diagnostic| (diagnostic.file, diagnostic.line, diagnostic.code))
    .collect();

  assert_eq!(
    codes,
    vec![
      ("C:/work/main.sag".to_string(), 12, "E0042".to_string()),
      ("a.sag".to_string(), 3, "E0001".to_string()),
      ("a.sag".to_string(), 4, "W0001".to_string())
    ]
  );
}
