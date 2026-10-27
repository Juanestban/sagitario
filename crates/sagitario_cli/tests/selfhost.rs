//! The self-hosted compiler in `compiler/` (docs/adr/0050-lexer-port-and-token-dumps.md): its
//! modules generated from the Rust compiler are up to date, `sagc`, built by `sag`, gives the
//! same results as `sag` on every Sagitario file of the repository, the bootstrap reaches its
//! fixed point (docs/adr/0056-bootstrap-and-acceptance.md), and `sagc lsp` answers the sessions of
//! `tests/lsp/` as they expect (docs/adr/0058-language-server.md).
//!
//! `SAG_BLESS=1 cargo test --test selfhost` rewrites the generated modules.

use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, MutexGuard};

fn root() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// The header of `compiler/src/diagnostics/codes.sag`, the source of the registry of codes.
const CODES_HEADER: &str = "//! The registry of diagnostic codes: every diagnostic the compiler emits uses one of these codes
//! (docs/adr/0067-bootstrap-from-the-seed.md).
//!
//! Errors: `E0001`–`E0099` lexical, `E0100`–`E0199` syntax, `E0200`–`E0299` names,
//! `E0300`–`E0399` types and control flow, `E0900`–`E0999` language features not implemented
//! yet. Warnings: `W0001`–`W0999`.
";

/// `compiler/src/diagnostics/codes.sag` as `sagitario_diagnostics::codes` registers its codes.
fn codes_of_rust() -> io::Result<String> {
  let source = fs::read_to_string(root().join("crates/sagitario_diagnostics/src/codes.rs"))?;
  let mut out = String::from(CODES_HEADER);
  let mut docs = Vec::new();

  for line in source.lines() {
    let line = line.trim();

    if let Some(doc) = line.strip_prefix("///") {
      docs.push(doc.to_string());
    } else if let Some(rest) = line.strip_prefix("pub const ").filter(|rest| rest.contains(": Code = Code(\"")) {
      let (name, code) = rest.split_once(": Code = Code(\"").unwrap_or_default();

      out.push('\n');

      for doc in docs.drain(..) {
        out.push_str(&format!("///{doc}\n"));
      }

      out.push_str(&format!("pub const {name}: String = \"{}\";\n", code.trim_end_matches("\");")));
    } else {
      docs.clear();
    }
  }

  Ok(out)
}

/// The ranges of the characters for which `test` holds.
fn ranges(test: fn(char) -> bool) -> Vec<(u32, u32)> {
  let mut ranges: Vec<(u32, u32)> = Vec::new();

  for code in 0..=0x10FFFFu32 {
    let Some(c) = char::from_u32(code) else { continue };

    if !test(c) {
      continue;
    }

    match ranges.last_mut() {
      Some((_, end)) if *end + 1 == code => *end = code,
      _ => ranges.push((code, code)),
    }
  }

  ranges
}

/// A test of `c` against sorted ranges, as nested comparisons.
fn tree(ranges: &[(u32, u32)], depth: usize) -> String {
  let indent = "  ".repeat(depth);

  if ranges.len() <= 4 {
    let tests: Vec<String> = ranges
      .iter()
      .map(|(start, end)| if start == end { format!("c == {start:#x}") } else { format!("(c >= {start:#x} && c <= {end:#x})") })
      .collect();
    let test = if tests.is_empty() { "false".to_string() } else { tests.join(" || ") };

    return format!("{indent}{test}\n");
  }

  let (low, high) = ranges.split_at(ranges.len() / 2);
  let split = high.first().map_or(0, |range| range.0);

  format!("{indent}if c < {split:#x} {{\n{}{indent}}} else {{\n{}{indent}}}\n", tree(low, depth + 1), tree(high, depth + 1))
}

/// A Unicode property of Rust's `char`, as a function of `compiler/src/unicode.sag`.
struct Property {
  name: &'static str,
  doc: &'static str,
  test: fn(char) -> bool,
}

/// `compiler/src/unicode.sag`: the Unicode properties of Rust's `char` that the lexer uses.
fn generated_unicode() -> String {
  let properties = [
    Property {
      name: "is_whitespace",
      doc: "Whether `c` has the property `White_Space`, like Rust's `char::is_whitespace`.",
      test: char::is_whitespace,
    },
    Property {
      name: "is_alphabetic",
      doc: "Whether `c` has the property `Alphabetic`, like Rust's `char::is_alphabetic`.",
      test: char::is_alphabetic,
    },
    Property {
      name: "is_alphanumeric",
      doc: "Whether `c` is alphabetic or numeric, like Rust's `char::is_alphanumeric`.",
      test: char::is_alphanumeric,
    },
  ];
  let (major, minor, update) = char::UNICODE_VERSION;
  let mut out = format!(
    "//! Unicode properties of characters, by code point, as Rust's `char` has them (Unicode {major}.{minor}.{update}).
//! Generated from Rust by the test `selfhost` (`SAG_BLESS=1 cargo test --test selfhost`) while the
//! Rust compiler exists, then kept as data (docs/adr/0067-bootstrap-from-the-seed.md).
"
  );

  for property in properties {
    out.push_str(&format!("\n/// {}\npub fn {}(c: u32) -> bool {{\n{}}}\n", property.doc, property.name, tree(&ranges(property.test), 1)));
  }

  out
}

/// Compares a generated module with the file, or rewrites the file with `SAG_BLESS=1`.
fn check_generated(relative: &str, expected: &str) -> io::Result<()> {
  let path = root().join(relative);

  if std::env::var("SAG_BLESS").is_ok_and(|value| value == "1") {
    return fs::write(path, expected);
  }

  let actual = fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");

  assert!(actual == expected, "`{relative}` is out of date: run `SAG_BLESS=1 cargo test --test selfhost`");

  Ok(())
}

/// `compiler/src/diagnostics/codes.sag` is the source of the codes, and the Rust compiler registers
/// the same ones until it retires (docs/adr/0067-bootstrap-from-the-seed.md).
#[test]
fn the_rust_compiler_registers_the_codes_of_codes_sag() -> io::Result<()> {
  let sagitario = fs::read_to_string(root().join("compiler/src/diagnostics/codes.sag"))?.replace("\r\n", "\n");

  assert!(
    sagitario == codes_of_rust()?,
    "`crates/sagitario_diagnostics/src/codes.rs` must register the codes of `compiler/src/diagnostics/codes.sag`, with the same names and documentation"
  );

  Ok(())
}

#[test]
fn the_unicode_module_is_up_to_date() -> io::Result<()> {
  check_generated("compiler/src/unicode.sag", &generated_unicode())
}

/// Every `.sag` file under `dir`, relative to the root and with `/` separators.
fn sources(dir: &Path, relative: &str, found: &mut Vec<String>) -> io::Result<()> {
  let mut entries: Vec<_> = fs::read_dir(dir)?.flatten().collect();

  entries.sort_by_key(|entry| entry.file_name());

  for entry in entries {
    let name = entry.file_name().to_string_lossy().to_string();
    let path = format!("{relative}/{name}");

    if entry.file_type()?.is_dir() {
      sources(&entry.path(), &path, found)?;
    } else if name.ends_with(".sag") {
      found.push(path);
    }
  }

  Ok(())
}

/// The exit code and the output of a command, with `\r\n` as `\n`: the C runtime writes
/// text-mode streams, which end lines with `\r\n` on Windows, and Rust's do not.
fn run(program: &Path, args: &[&str]) -> io::Result<(Option<i32>, String, String)> {
  let output: Output = Command::new(program).args(args).current_dir(root()).env("NO_COLOR", "1").output()?;
  let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).replace("\r\n", "\n");

  Ok((output.status.code(), text(&output.stdout), text(&output.stderr)))
}

/// Both tests that build `sagc` write it to `compiler/src/`, so they take turns.
static BUILDING: Mutex<()> = Mutex::new(());

/// The turn to build `sagc`, also after a test that held it failed.
fn building() -> MutexGuard<'static, ()> {
  BUILDING.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// What `sag build compiler/src/main.sag` writes next to the source: the executable, and with
/// the MSVC linker its incremental-link and debug files. Removed on drop, also when the test
/// fails.
struct BuiltSagc {
  /// The path without extension.
  base: PathBuf,
}

impl Drop for BuiltSagc {
  fn drop(&mut self) {
    for extension in ["", "exe", "ilk", "pdb"] {
      let _ = fs::remove_file(self.base.with_extension(extension));
    }
  }
}

#[test]
fn sagc_dumps_the_same_results() -> io::Result<()> {
  let _turn = building();
  let sag = PathBuf::from(env!("CARGO_BIN_EXE_sag"));
  let built = BuiltSagc { base: root().join("compiler/src/main") };
  let (code, _, stderr) = run(&sag, &["build", "compiler/src/main.sag"])?;
  let sagc = built.base.with_extension(std::env::consts::EXE_EXTENSION);

  assert!(code == Some(0), "building `sagc` failed:\n{stderr}");

  let mut files = Vec::new();

  for dir in ["tests", "compiler"] {
    sources(&root().join(dir), dir, &mut files)?;
  }

  let mut differences = Vec::new();

  // `types` contains every table and diagnostic of `declarations`, which stays for debugging.
  let dumps = ["tokens", "ast", "resolve", "types", "c"];

  for file in &files {
    for (dump, format) in dumps.iter().flat_map(|dump| ["human", "short"].map(|format| (dump, format))) {
      let dump_flag = format!("--dump={dump}");
      let args = ["check", dump_flag.as_str(), "--diagnostic-format", format, file.as_str()];
      let expected = run(&sag, &args)?;
      let actual = run(&sagc, &args)?;

      if expected != actual {
        differences.push(format!(
          "{file} ({dump}, {format}): exit {:?} vs {:?}\n--- sag stderr\n{}\n--- sagc stderr\n{}\n--- first differing stdout line\n{}",
          expected.0,
          actual.0,
          expected.2,
          actual.2,
          first_difference(&expected.1, &actual.1),
        ));
      }
    }
  }

  assert!(
    differences.is_empty(),
    "{} of {} runs differ:\n\n{}",
    differences.len(),
    files.len() * dumps.len() * 2,
    differences.iter().take(5).cloned().collect::<Vec<_>>().join("\n\n")
  );

  Ok(())
}

/// A copy of stage 1 outside `compiler/src/`, where stage 1 writes stage 2: Windows cannot
/// replace a running executable. Removed on drop.
struct Stage1 {
  dir: PathBuf,
}

impl Drop for Stage1 {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.dir);
  }
}

/// Stage 0 (`sag`) builds stage 1 (`sagc`), stage 1 builds stage 2, and both generate the same C
/// for the compiler (docs/adr/0049-self-hosting-plan-and-language-prerequisites.md, Q213).
#[test]
fn bootstrap_reaches_the_fixed_point() -> io::Result<()> {
  let _turn = building();
  let sag = PathBuf::from(env!("CARGO_BIN_EXE_sag"));
  let built = BuiltSagc { base: root().join("compiler/src/main") };
  let executable = built.base.with_extension(std::env::consts::EXE_EXTENSION);
  let (code, _, stderr) = run(&sag, &["build", "compiler/src/main.sag"])?;

  assert!(code == Some(0), "building stage 1 failed:\n{stderr}");

  let stage1 = Stage1 { dir: std::env::temp_dir().join(format!("sag-stage1-{}", std::process::id())) };

  fs::create_dir_all(&stage1.dir)?;

  let stage1_exe = stage1.dir.join("sagc").with_extension(std::env::consts::EXE_EXTENSION);

  fs::copy(&executable, &stage1_exe)?;

  let (code, _, stderr) = run(&stage1_exe, &["build", "compiler/src/main.sag"])?;

  assert!(code == Some(0), "stage 1 failed to build stage 2:\n{stderr}");

  let args = ["check", "--dump=c", "compiler/src/main.sag"];
  let from_stage1 = run(&stage1_exe, &args)?;
  let from_stage2 = run(&executable, &args)?;

  assert!(from_stage1.0 == Some(0), "stage 1 failed to generate the C of the compiler:\n{}", from_stage1.2);
  assert!(
    from_stage1 == from_stage2,
    "stages 1 and 2 generate different C for the compiler (exit {:?} vs {:?}):\n{}\n{}",
    from_stage1.0,
    from_stage2.0,
    from_stage2.2,
    first_difference(&from_stage1.1, &from_stage2.1)
  );

  Ok(())
}

/// The repository's root without `..`, for the paths `sagc lsp` reports.
fn repository() -> PathBuf {
  let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));

  manifest.parent().and_then(Path::parent).map(Path::to_path_buf).unwrap_or_else(root)
}

/// The URI of a file as `sagc lsp` writes them: `/` as the separator, a Windows drive in lowercase
/// with its colon encoded, and every byte but letters, digits and `/-._~` encoded.
fn file_uri(path: &Path) -> String {
  let mut text = path.display().to_string().replace('\\', "/");

  if cfg!(windows) && text.as_bytes().get(1) == Some(&b':') {
    text = format!("/{}{}", text.get(..1).unwrap_or_default().to_ascii_lowercase(), text.get(1..).unwrap_or_default());
  }

  let mut uri = String::from("file://");

  for byte in text.bytes() {
    if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
      uri.push(char::from(byte));
    } else {
      uri.push_str(&format!("%{byte:02X}"));
    }
  }

  uri
}

/// A JSON string holding `text`.
fn json_string(text: &str) -> String {
  let mut out = String::from("\"");

  for c in text.chars() {
    match c {
      '"' => out.push_str("\\\""),
      '\\' => out.push_str("\\\\"),
      '\n' => out.push_str("\\n"),
      '\r' => out.push_str("\\r"),
      '\t' => out.push_str("\\t"),
      c if u32::from(c) < 32 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
      c => out.push(c),
    }
  }

  out.push('"');
  out
}

/// A message of a session with its placeholders replaced: `{uri}` by the document's URI, `{root}`
/// by the URI of the repository, and `{text:name}` by the text of `tests/lsp/name` as a JSON
/// string.
fn expand(line: &str, uri: &str, root: &str, dir: &Path) -> io::Result<String> {
  let mut out = line.replace("{uri}", uri).replace("{root}", root);

  while let Some(start) = out.find("{text:") {
    let end = out.get(start..).and_then(|rest| rest.find('}')).map_or(out.len(), |end| start + end);
    let name = out.get(start + 6..end).unwrap_or_default().to_string();
    let text = fs::read_to_string(dir.join(&name))?;

    out.replace_range(start..=end.min(out.len() - 1), &json_string(&text));
  }

  Ok(out)
}

/// The bodies of the messages of an LSP output.
fn messages(mut output: &[u8]) -> Vec<String> {
  let mut bodies = Vec::new();

  while let Some(split) = output.windows(4).position(|window| window == b"\r\n\r\n") {
    let header = String::from_utf8_lossy(output.get(..split).unwrap_or_default()).to_string();
    let length: usize = header.split(':').nth(1).and_then(|value| value.trim().parse().ok()).unwrap_or(0);
    let body = output.get(split + 4..split + 4 + length).unwrap_or_default();

    bodies.push(String::from_utf8_lossy(body).to_string());
    output = output.get(split + 4 + length..).unwrap_or_default();
  }

  bodies
}

/// Each `tests/lsp/<case>.session` sends its messages (one per line, `raw ` for a body sent as
/// written) to `sagc lsp` about `<case>.sag`, and `<case>.expected` holds what the server
/// answers, one message per line, with `{uri}` for the document, `{dir}` for its directory and
/// `{root}` for the repository; `SAG_BLESS=1` rewrites it.
#[test]
fn language_server_answers_the_sessions() -> io::Result<()> {
  let _turn = building();
  let sag = PathBuf::from(env!("CARGO_BIN_EXE_sag"));
  let built = BuiltSagc { base: root().join("compiler/src/main") };
  let (code, _, stderr) = run(&sag, &["build", "compiler/src/main.sag"])?;
  let sagc = built.base.with_extension(std::env::consts::EXE_EXTENSION);

  assert!(code == Some(0), "building `sagc` failed:\n{stderr}");

  let dir = repository().join("tests").join("lsp");
  let root_uri = file_uri(&repository());
  let mut sessions: Vec<PathBuf> =
    fs::read_dir(&dir)?.flatten().map(|entry| entry.path()).filter(|path| path.extension().is_some_and(|ext| ext == "session")).collect();

  sessions.sort();
  assert!(!sessions.is_empty(), "no sessions in `tests/lsp`");

  for session in sessions {
    let name = session.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let uri = file_uri(&session.with_extension("sag"));
    let mut input = Vec::new();

    for line in fs::read_to_string(&session)?.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with("//")) {
      let body = match line.strip_prefix("raw ") {
        Some(raw) => raw.to_string(),
        None => expand(line, &uri, &root_uri, &dir)?,
      };

      input.extend(format!("Content-Length: {}\r\n\r\n{body}", body.len()).bytes());
    }

    let mut child = Command::new(&sagc).arg("lsp").current_dir(root()).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    // Written by another thread, so a server answering while it reads never blocks on a full pipe.
    let writer = child.stdin.take().map(|mut stdin| std::thread::spawn(move || stdin.write_all(&input)));
    let output = child.wait_with_output()?;

    if let Some(handle) = writer {
      let _ = handle.join();
    }

    assert!(output.status.code() == Some(0), "`sagc lsp` on {name}: exit {:?}\n{}", output.status.code(), String::from_utf8_lossy(&output.stderr));

    // Other files of the directory, such as the modules a session goes to.
    let dir_uri = file_uri(&dir) + "/";
    let actual: String = messages(&output.stdout)
      .iter()
      .map(|body| body.replace(&uri, "{uri}").replace(&dir_uri, "{dir}/").replace(&root_uri, "{root}") + "\n")
      .collect();

    check_generated(&format!("tests/lsp/{name}.expected"), &actual)?;
  }

  Ok(())
}

fn first_difference(expected: &str, actual: &str) -> String {
  for (number, pair) in expected.lines().zip(actual.lines()).enumerate() {
    if pair.0 != pair.1 {
      return format!("line {}: sag `{}`, sagc `{}`", number + 1, pair.0, pair.1);
    }
  }

  format!("lengths: sag {} lines, sagc {} lines", expected.lines().count(), actual.lines().count())
}
