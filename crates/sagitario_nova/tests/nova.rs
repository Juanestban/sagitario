//! End-to-end tests of the `nova` binary: each one works in a new temporary directory. Building
//! and running need a C compiler, found like `sag` finds it (`SAG_CC`, then `clang`, `gcc`).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

/// A temporary directory, removed on drop.
struct TempDir {
  path: PathBuf,
}

impl TempDir {
  fn new() -> io::Result<TempDir> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("nova-test-{}-{count}", std::process::id()));
    let _ = fs::remove_dir_all(&path);

    fs::create_dir_all(&path)?;

    Ok(TempDir { path })
  }
}

impl Drop for TempDir {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.path);
  }
}

fn nova(dir: &Path, args: &[&str]) -> io::Result<Output> {
  Command::new(env!("CARGO_BIN_EXE_nova")).args(args).current_dir(dir).output()
}

fn stdout(output: &Output) -> String {
  String::from_utf8_lossy(&output.stdout).replace('\r', "")
}

fn stderr(output: &Output) -> String {
  String::from_utf8_lossy(&output.stderr).to_string()
}

fn assert_success(output: &Output) {
  assert!(output.status.success(), "nova failed with {:?}\nstderr:\n{}", output.status.code(), stderr(output));
}

fn exe(path: PathBuf) -> PathBuf {
  path.with_extension(std::env::consts::EXE_EXTENSION)
}

#[test]
fn creates_builds_runs_and_cleans_a_program() -> io::Result<()> {
  let temp = TempDir::new()?;

  assert_success(&nova(&temp.path, &["new", "hello"])?);

  let package = temp.path.join("hello");

  assert_eq!(fs::read_to_string(package.join("Nova.toml"))?, "[package]\nname = \"hello\"\nversion = \"0.1.0\"\n");
  assert_eq!(fs::read_to_string(package.join(".gitignore"))?, "/target\n");
  assert!(package.join("src").join("main.sag").is_file());

  let run = nova(&package, &["run"])?;

  assert_success(&run);
  assert_eq!(stdout(&run), "Hello, world!\n");

  assert_success(&nova(&package, &["build"])?);
  assert!(exe(package.join("target").join("debug").join("hello")).is_file());

  assert_success(&nova(&package, &["build", "--release", "--emit-c"])?);
  assert!(exe(package.join("target").join("release").join("hello")).is_file());
  assert!(package.join("target").join("release").join("hello.c").is_file());

  assert_success(&nova(&package, &["clean"])?);
  assert!(!package.join("target").exists());

  Ok(())
}

#[test]
fn runs_with_arguments_modules_and_exit_codes() -> io::Result<()> {
  let temp = TempDir::new()?;

  assert_success(&nova(&temp.path, &["new", "tool"])?);

  let package = temp.path.join("tool");
  let util = package.join("src").join("util");

  fs::create_dir_all(&util)?;
  fs::write(util.join("text.sag"), "pub fn shout(text: String) -> String {\n  text.to_ascii_uppercase()\n}\n")?;
  fs::write(
    package.join("src").join("main.sag"),
    "use std::env;\nuse std::process;\nuse pkg::util::text;\n\nfn main() {\n  const args = env::args();\n  println(text::shout(args[1]));\n  process::exit(args.len() as i32);\n}\n",
  )?;

  let run = nova(&package, &["run", "--", "hola", "x"])?;

  assert_eq!(stdout(&run), "HOLA\n");
  assert_eq!(run.status.code(), Some(3), "stderr:\n{}", stderr(&run));

  Ok(())
}

#[test]
fn finds_the_package_from_a_subdirectory() -> io::Result<()> {
  let temp = TempDir::new()?;

  assert_success(&nova(&temp.path, &["new", "app"])?);

  let check = nova(&temp.path.join("app").join("src"), &["check"])?;

  assert_success(&check);
  assert!(stderr(&check).contains("Checking app v0.1.0"), "{}", stderr(&check));

  Ok(())
}

#[test]
fn checks_a_library_without_building_or_running_it() -> io::Result<()> {
  let temp = TempDir::new()?;

  assert_success(&nova(&temp.path, &["new", "--lib", "geometry"])?);

  let package = temp.path.join("geometry");

  assert!(package.join("src").join("lib.sag").is_file());

  // A library needs no `main`, and a function called `main` is an ordinary one.
  fs::write(package.join("src").join("lib.sag"), "pub fn main(x: i32) -> i32 {\n  x\n}\n")?;

  let build = nova(&package, &["build"])?;

  assert_success(&build);
  assert!(stderr(&build).contains("is a library"), "{}", stderr(&build));
  assert!(!package.join("target").exists());

  let run = nova(&package, &["run"])?;

  assert_eq!(run.status.code(), Some(1));
  assert!(stderr(&run).contains("cannot run"), "{}", stderr(&run));

  fs::write(package.join("src").join("lib.sag"), "pub fn broken() -> i32 {\n  \"text\"\n}\n")?;

  let broken = nova(&package, &["check", "--diagnostic-format", "short"])?;

  assert_eq!(broken.status.code(), Some(1));
  assert!(stderr(&broken).contains("lib.sag:2:3: error[E0300]"), "{}", stderr(&broken));

  Ok(())
}

#[test]
fn reports_errors_in_the_program() -> io::Result<()> {
  let temp = TempDir::new()?;

  assert_success(&nova(&temp.path, &["new", "app"])?);

  let package = temp.path.join("app");

  fs::write(package.join("src").join("main.sag"), "fn helper() {}\n")?;

  let check = nova(&package, &["check", "--diagnostic-format", "short"])?;

  assert_eq!(check.status.code(), Some(1));
  assert!(stderr(&check).contains("error[E0203]"), "{}", stderr(&check));

  Ok(())
}

#[test]
fn init_keeps_existing_files() -> io::Result<()> {
  let temp = TempDir::new()?;
  let src = temp.path.join("src");
  let main = "fn main() {\n  println(\"mine\");\n}\n";

  fs::create_dir_all(&src)?;
  fs::write(src.join("main.sag"), main)?;
  fs::write(temp.path.join(".gitignore"), "custom\n")?;

  // The temporary directory's name is not a package name.
  let unnamed = nova(&temp.path, &["init"])?;

  assert_eq!(unnamed.status.code(), Some(1));
  assert!(stderr(&unnamed).contains("--name"), "{}", stderr(&unnamed));

  assert_success(&nova(&temp.path, &["init", "--name", "mine", "--lib"])?);
  assert_eq!(fs::read_to_string(src.join("main.sag"))?, main);
  assert_eq!(fs::read_to_string(temp.path.join(".gitignore"))?, "custom\n");
  assert!(!src.join("lib.sag").exists());
  assert!(fs::read_to_string(temp.path.join("Nova.toml"))?.contains("name = \"mine\""));

  let run = nova(&temp.path, &["run"])?;

  assert_eq!(stdout(&run), "mine\n");

  let again = nova(&temp.path, &["init", "--name", "mine"])?;

  assert_eq!(again.status.code(), Some(1));

  Ok(())
}

#[test]
fn new_rejects_invalid_names_and_existing_directories() -> io::Result<()> {
  let temp = TempDir::new()?;

  for name in ["My-App", "fn", "std", "1app"] {
    let output = nova(&temp.path, &["new", name])?;

    assert_eq!(output.status.code(), Some(1), "{name}");
    assert!(!temp.path.join(name).exists(), "{name}");
  }

  assert_success(&nova(&temp.path, &["new", "app"])?);
  assert_eq!(nova(&temp.path, &["new", "app"])?.status.code(), Some(1));

  Ok(())
}

#[test]
fn reports_a_missing_manifest() -> io::Result<()> {
  let temp = TempDir::new()?;

  for command in ["build", "run", "check", "clean"] {
    let output = nova(&temp.path, &[command])?;

    assert_eq!(output.status.code(), Some(1), "{command}");
    assert!(stderr(&output).contains("cannot find `Nova.toml`"), "{}", stderr(&output));
  }

  Ok(())
}

#[test]
fn reports_usage_errors() -> io::Result<()> {
  let temp = TempDir::new()?;

  assert_eq!(nova(&temp.path, &["publish"])?.status.code(), Some(2));
  assert_eq!(nova(&temp.path, &["build", "main.sag"])?.status.code(), Some(2));
  assert_success(&nova(&temp.path, &["--version"])?);

  Ok(())
}

/// Each manifest and the error it causes, as `line:column: error[code]`.
const MANIFEST_ERRORS: [(&str, &str); 10] = [
  ("[package]\nname = \"app\"\nversion = 0.1\n", "3:11: error[N0001]"),
  ("[package]\nname = \"app\"\nname = \"other\"\nversion = \"0.1.0\"\n", "3:1: error[N0002]"),
  ("[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n", "4:1: error[N0003]"),
  ("[package]\nname = \"app\"\n", "1:1: error[N0004]"),
  ("[package]\nname = \"app\"\nversion = 1\n", "3:11: error[N0005]"),
  ("[package]\nname = \"My-App\"\nversion = \"0.1.0\"\n", "2:8: error[N0006]"),
  ("[package]\nname = \"app\"\nversion = \"1.0\"\n", "3:11: error[N0007]"),
  ("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\n", "5:1: error[N0008]"),
  // The layout errors point at `[package]`: no source file, then both.
  ("[package]\nname = \"app\"\nversion = \"0.1.0\"\n", "1:1: error[N0009]"),
  ("[package]\nname = \"app\"\nversion = \"0.1.0\"\n", "1:1: error[N0009]"),
];

#[test]
fn reports_manifest_errors_with_their_location() -> io::Result<()> {
  for (index, (manifest, expected)) in MANIFEST_ERRORS.iter().enumerate() {
    let temp = TempDir::new()?;
    let src = temp.path.join("src");

    fs::create_dir_all(&src)?;
    fs::write(temp.path.join("Nova.toml"), manifest)?;

    // The first layout case has no source file; every other one has `src/main.sag`.
    if index != 8 {
      fs::write(src.join("main.sag"), "fn main() {}\n")?;
    }

    if index == 9 {
      fs::write(src.join("lib.sag"), "pub fn f() {}\n")?;
    }

    let output = nova(&temp.path, &["check", "--diagnostic-format", "short"])?;
    let reported = stderr(&output);

    assert_eq!(output.status.code(), Some(1), "{manifest}");
    assert!(reported.contains(&format!("Nova.toml:{expected}")), "expected `{expected}` for:\n{manifest}\nreported:\n{reported}");
  }

  Ok(())
}

#[test]
fn every_nova_code_has_a_test() -> io::Result<()> {
  let tests: String = MANIFEST_ERRORS.iter().map(|(_, expected)| *expected).collect();

  for code in sagitario_nova::codes::ALL {
    assert!(tests.contains(code.0), "no test reports {}", code.0);
  }

  Ok(())
}
