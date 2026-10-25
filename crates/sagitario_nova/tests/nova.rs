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
fn builds_but_does_not_run_a_library() -> io::Result<()> {
  let temp = TempDir::new()?;

  assert_success(&nova(&temp.path, &["new", "--lib", "geometry"])?);

  let package = temp.path.join("geometry");

  assert!(package.join("src").join("lib.sag").is_file());

  // A library needs no `main`, and a function called `main` is an ordinary one.
  fs::write(package.join("src").join("lib.sag"), "pub fn main(x: i32) -> i32 {\n  x\n}\n")?;

  // Without exported functions it still builds, with a warning.
  let build = nova(&package, &["build"])?;

  assert_success(&build);
  assert!(stderr(&build).contains("exports no functions"), "{}", stderr(&build));
  assert!(package.join("target").join("debug").join("geometry.h").is_file());

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
const MANIFEST_ERRORS: [(&str, &str); 13] = [
  ("[package]\nname = \"app\"\nversion = 0.1\n", "3:11: error[N0001]"),
  ("[package]\nname = \"app\"\nname = \"other\"\nversion = \"0.1.0\"\n", "3:1: error[N0002]"),
  ("[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n", "4:1: error[N0003]"),
  ("[package]\nname = \"app\"\n", "1:1: error[N0004]"),
  ("[package]\nname = \"app\"\nversion = 1\n", "3:11: error[N0005]"),
  ("[package]\nname = \"My-App\"\nversion = \"0.1.0\"\n", "2:8: error[N0006]"),
  ("[package]\nname = \"app\"\nversion = \"1.0\"\n", "3:11: error[N0007]"),
  ("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nutil = { git = \"https://example.com/util\" }\n", "6:10: error[N0008]"),
  ("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[library]\noutputs = [\"shared\"]\n", "6:12: error[N0014]"),
  ("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[library]\n", "5:1: error[N0015]"),
  ("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[link]\nsources = [\"c/missing.c\"]\n", "6:12: error[N0016]"),
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
    if index != MANIFEST_ERRORS.len() - 2 {
      fs::write(src.join("main.sag"), "fn main() {}\n")?;
    }

    if index == MANIFEST_ERRORS.len() - 1 {
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
  let tests: String = MANIFEST_ERRORS.iter().chain(DEPENDENCY_ERRORS.iter()).map(|(_, expected)| *expected).collect();

  for code in sagitario_nova::codes::ALL {
    assert!(tests.contains(code.0), "no test reports {}", code.0);
  }

  Ok(())
}

/// Writes a package with `Nova.toml` and the given source files in `dir`.
fn package(dir: &Path, manifest: &str, files: &[(&str, &str)]) -> io::Result<()> {
  fs::create_dir_all(dir.join("src"))?;
  fs::write(dir.join("Nova.toml"), manifest)?;

  for (name, text) in files {
    let file = dir.join("src").join(name);

    if let Some(parent) = file.parent() {
      fs::create_dir_all(parent)?;
    }

    fs::write(file, text)?;
  }

  Ok(())
}

fn manifest(name: &str, dependencies: &[(&str, &str)]) -> String {
  let mut text = format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n");

  if !dependencies.is_empty() {
    text.push_str("\n[dependencies]\n");

    for (dependency, path) in dependencies {
      text.push_str(&format!("{dependency} = {{ path = \"{path}\" }}\n"));
    }
  }

  text
}

#[test]
fn builds_a_program_with_dependencies() -> io::Result<()> {
  let temp = TempDir::new()?;

  // `app` uses `geo` and `numbers`; `geo` uses `numbers` too, which is compiled once.
  package(
    &temp.path.join("numbers"),
    &manifest("numbers", &[]),
    &[("lib.sag", "pub fn square(x: i32) -> i32 {\n  x * x\n}\n\n/// An ordinary function in a library.\npub fn main() -> i32 {\n  7\n}\n")],
  )?;
  package(
    &temp.path.join("geo"),
    &manifest("geo", &[("numbers", "../numbers")]),
    &[
      ("lib.sag", "use pkg::shapes::square::Square;\n\npub fn unit() -> Square {\n  Square { side: 1 }\n}\n"),
      ("shapes/square.sag", "pub struct Square {\n  pub side: i32;\n\n  pub fn area(self) -> i32 {\n    numbers::square(self.side)\n  }\n}\n"),
    ],
  )?;
  package(
    &temp.path.join("app"),
    &manifest("app", &[("geo", "../geo"), ("numbers", "../numbers")]),
    &[(
      "main.sag",
      "use geo::shapes::square::Square;\n\nfn main() {\n  const big = Square { side: 4 };\n  println(`${big.area()} ${geo::unit().area()} ${numbers::main()}`);\n}\n",
    )],
  )?;

  let run = nova(&temp.path.join("app"), &["run"])?;

  assert_success(&run);
  assert_eq!(stdout(&run), "16 1 7\n");
  assert!(stderr(&run).contains("Using geo v0.1.0"), "{}", stderr(&run));

  // A library with dependencies is checked with them.
  assert_success(&nova(&temp.path.join("geo"), &["check"])?);

  Ok(())
}

#[test]
fn keeps_items_private_across_packages_and_lets_local_names_win() -> io::Result<()> {
  let temp = TempDir::new()?;

  package(&temp.path.join("util"), &manifest("util", &[]), &[("lib.sag", "pub fn open() -> i32 {\n  1\n}\n\nfn hidden() -> i32 {\n  2\n}\n")])?;
  package(&temp.path.join("app"), &manifest("app", &[("util", "../util")]), &[("main.sag", "fn main() {\n  util::open();\n  util::hidden();\n}\n")])?;

  let check = nova(&temp.path.join("app"), &["check", "--diagnostic-format", "short"])?;

  assert_eq!(check.status.code(), Some(1));
  assert!(stderr(&check).contains("main.sag:3:9: error[E0207]"), "{}", stderr(&check));

  // An import called like a dependency hides it.
  fs::write(temp.path.join("app").join("src").join("helpers.sag"), "pub fn open() -> i32 {\n  3\n}\n")?;
  fs::write(temp.path.join("app").join("src").join("main.sag"), "use pkg::helpers as util;\n\nfn main() {\n  println(`${util::open()}`);\n}\n")?;

  let run = nova(&temp.path.join("app"), &["run"])?;

  assert_eq!(stdout(&run), "3\n", "{}", stderr(&run));

  Ok(())
}

/// The dependencies of `app`, and the error each case causes in a `Nova.toml`.
const DEPENDENCY_ERRORS: [(&str, &str); 4] =
  [("missing", "6:16: error[N0010]"), ("renamed", "6:1: error[N0011]"), ("program", "6:1: error[N0012]"), ("cycle", "6:1: error[N0013]")];

#[test]
fn reports_dependency_errors() -> io::Result<()> {
  for (case, expected) in DEPENDENCY_ERRORS {
    let temp = TempDir::new()?;
    let app = temp.path.join("app");
    let main = [("main.sag", "fn main() {}\n")];
    let lib = [("lib.sag", "pub fn f() {}\n")];

    // Each case's `app` depends on `dep` at `../dep`; its line 6 is the dependency.
    package(&app, &manifest("app", &[("dep", "../dep")]), &main)?;

    match case {
      "renamed" => package(&temp.path.join("dep"), &manifest("other", &[]), &lib)?,
      "program" => package(&temp.path.join("dep"), &manifest("dep", &[]), &main)?,
      "cycle" => package(&temp.path.join("dep"), &manifest("dep", &[("app", "../app")]), &lib)?,
      _ => {}
    }

    let output = nova(&app, &["check", "--diagnostic-format", "short"])?;
    let reported = stderr(&output);

    assert_eq!(output.status.code(), Some(1), "{case}");
    assert!(reported.contains(&format!("Nova.toml:{expected}")), "expected `{expected}` for {case}, reported:\n{reported}");

    // The cycle closes in `dep`'s manifest, which points back at `app`.
    if case == "cycle" {
      assert!(reported.contains("app -> dep -> app"), "{reported}");
    }
  }

  Ok(())
}

/// The C compiler the tests build C programs with: `SAG_CC`, else `clang`, else `gcc`, as `sag`
/// chooses it.
fn c_compiler() -> String {
  if let Ok(compiler) = std::env::var("SAG_CC")
    && !compiler.trim().is_empty()
  {
    return compiler;
  }

  let works = |command: &str| Command::new(command).arg("--version").output().is_ok_and(|output| output.status.success());

  if works("clang") { "clang".to_string() } else { "gcc".to_string() }
}

/// Whether the C compiler is MSVC's `cl`, which `sag` recognizes by its name.
fn is_cl(compiler: &str) -> bool {
  Path::new(compiler).file_stem().is_some_and(|stem| stem.eq_ignore_ascii_case("cl"))
}

/// The target triple of the C compiler.
fn c_target(compiler: &str) -> io::Result<String> {
  if is_cl(compiler) {
    return Ok(format!("{}-pc-windows-msvc", std::env::consts::ARCH));
  }

  let output = Command::new(compiler).arg("-dumpmachine").output()?;

  Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Compiles the C program `source` in `dir` into `executable`, linked with `libraries`.
fn compile_c_program(compiler: &str, dir: &Path, source: &str, libraries: &[String], executable: &str) -> io::Result<Output> {
  let mut command = Command::new(compiler);

  if is_cl(compiler) {
    command.args(["/nologo", "/std:c11", source]).args(libraries).arg(format!("/Fe{executable}"));
  } else {
    command.args(["-std=c11", source]).args(libraries).arg("-o").arg(executable);
  }

  // `cl` prints its errors to stdout.
  let mut output = command.current_dir(dir).output()?;

  output.stderr.splice(0..0, output.stdout.iter().copied());

  Ok(output)
}

const LIBRARY_SOURCE: &str = "use std::ffi::c_int;

extern \"C\" {
  fn helper_twice(x: c_int) -> c_int;
}

/// A point in the plane.
@repr(C)
pub struct Point {
  pub x: f64;
  pub y: f64;
}

/// The squared distance between
/// two points.
@export(\"geo_distance2\")
pub extern \"C\" fn distance2(a: Point, b: Point) -> f64 {
  const dx = a.x - b.x;
  const dy = a.y - b.y;
  dx * dx + dy * dy
}

/// Doubles `x` in C, after using a list.
pub extern \"C\" fn geo_twice(x: i32) -> i32 {
  const items = List::from([1, 2, 3]);
  unsafe { helper_twice(x) } + items.len() as i32 - 3
}

pub extern \"C\" fn geo_fill(out: *mut Point) {
  unsafe { *out = Point { x: 1.5, y: 2.5 }; }
}
";

const HOST_SOURCE: &str = "#include <stdio.h>
#include \"geo.h\"

int main(void) {
  Point a = {0.0, 0.0};
  Point b = {3.0, 4.0};
  Point c;
  geo_fill(&c);
  printf(\"%.1f %d %.1f %.1f\\n\", geo_distance2(a, b), geo_twice(21), c.x, c.y);
  return 0;
}
";

#[test]
fn builds_a_library_that_c_programs_use() -> io::Result<()> {
  let temp = TempDir::new()?;
  let geo = temp.path.join("geo");

  package(&geo, &format!("{}\n[link]\nsources = [\"c/helper.c\"]\n", manifest("geo", &[])), &[("lib.sag", LIBRARY_SOURCE)])?;
  fs::create_dir_all(geo.join("c"))?;
  fs::write(geo.join("c").join("helper.c"), "int helper_twice(int x) {\n  return 2 * x;\n}\n")?;

  let build = nova(&geo, &["build"])?;

  assert_success(&build);

  let target = geo.join("target").join("debug");
  let header = fs::read_to_string(target.join("geo.h"))?;

  for expected in [
    "#ifndef GEO_H",
    "struct Point {\n  double x;\n  double y;\n};",
    "/* The squared distance between\n * two points. */\ndouble geo_distance2(Point a, Point b);",
    "int32_t geo_twice(int32_t x);",
    "void geo_fill(Point *out);",
    "extern \"C\" {",
  ] {
    assert!(header.contains(expected), "`{expected}` is not in the header:\n{header}");
  }

  let compiler = c_compiler();
  let target_triple = c_target(&compiler)?;
  let windows = target_triple.contains("windows") || target_triple.contains("mingw");
  let msvc = windows && !target_triple.contains("windows-gnu") && !target_triple.contains("mingw");
  let (static_lib, import_lib) = match (windows, msvc) {
    (true, true) => ("geo.lib", "geo.dll.lib"),
    (true, false) => ("libgeo.a", "libgeo.dll.a"),
    _ => ("libgeo.a", ""),
  };

  fs::write(target.join("host.c"), HOST_SOURCE)?;

  // The runtime uses libm outside Windows; the dynamic library finds its own.
  let libm = if windows { None } else { Some("-lm".to_string()) };
  let dynamic: Vec<String> =
    if windows { vec![import_lib.to_string()] } else { vec!["-L.".to_string(), "-lgeo".to_string(), format!("-Wl,-rpath,{}", target.display())] };

  for (exe, mut libraries) in [("host_static", vec![static_lib.to_string()]), ("host_dynamic", dynamic)] {
    let executable = exe_name(exe);

    libraries.extend(libm.clone());

    let compile = compile_c_program(&compiler, &target, "host.c", &libraries, &executable)?;

    assert!(compile.status.success(), "{exe}: {}", String::from_utf8_lossy(&compile.stderr));

    let run = Command::new(target.join(&executable)).current_dir(&target).output()?;

    assert_eq!(stdout(&run), "25.0 42 1.5 2.5\n", "{exe}: {}", String::from_utf8_lossy(&run.stderr));
  }

  Ok(())
}

fn exe_name(stem: &str) -> String {
  format!("{stem}{}", std::env::consts::EXE_SUFFIX)
}

#[test]
fn builds_only_the_chosen_library_outputs() -> io::Result<()> {
  let temp = TempDir::new()?;
  let lib = temp.path.join("solo");

  package(
    &lib,
    &format!("{}\n[library]\noutputs = [\"static\"]\n", manifest("solo", &[])),
    &[("lib.sag", "pub extern \"C\" fn solo_one() -> i32 {\n  1\n}\n")],
  )?;
  assert_success(&nova(&lib, &["build", "--release"])?);

  let files: Vec<String> =
    fs::read_dir(lib.join("target").join("release"))?.flatten().map(|entry| entry.file_name().to_string_lossy().to_string()).collect();

  assert!(files.iter().any(|file| file == "solo.h"), "{files:?}");
  assert!(files.iter().any(|file| file == "solo.lib" || file == "libsolo.a"), "{files:?}");
  assert!(!files.iter().any(|file| file.ends_with(".dll") || file.ends_with(".so") || file.ends_with(".dylib")), "{files:?}");

  // Libraries cannot be sanitized, and one without exports is reported.
  assert_eq!(nova(&lib, &["build", "--sanitize"])?.status.code(), Some(1));

  fs::write(lib.join("src").join("lib.sag"), "pub fn inner() -> i32 {\n  1\n}\n")?;

  let empty = nova(&lib, &["build"])?;

  assert_success(&empty);
  assert!(stderr(&empty).contains("exports no functions"), "{}", stderr(&empty));

  Ok(())
}

#[test]
fn links_c_sources_and_libraries_into_programs() -> io::Result<()> {
  let temp = TempDir::new()?;
  let app = temp.path.join("app");
  let libraries = if cfg!(windows) { "" } else { "libraries = [\"m\"]\n" };

  package(
    &app,
    &format!("{}\n[link]\n{libraries}sources = [\"c/add.c\"]\n", manifest("app", &[])),
    &[(
      "main.sag",
      "use std::ffi::c_int;\n\nextern \"C\" {\n  fn c_add(a: c_int, b: c_int) -> c_int;\n}\n\nfn main() {\n  const sum = unsafe { c_add(40, 2) };\n  println(`${sum}`);\n}\n",
    )],
  )?;
  fs::create_dir_all(app.join("c"))?;
  fs::write(app.join("c").join("add.c"), "int c_add(int a, int b) {\n  return a + b;\n}\n")?;

  let run = nova(&app, &["run"])?;

  assert_eq!(stdout(&run), "42\n", "{}", stderr(&run));

  // A C source that does not compile is an error of the package, not of the compiler.
  fs::write(app.join("c").join("add.c"), "int c_add(int a, int b) {\n  return a +;\n}\n")?;

  let broken = nova(&app, &["build"])?;

  assert_eq!(broken.status.code(), Some(1));
  assert!(stderr(&broken).contains("cannot compile the C source"), "{}", stderr(&broken));

  Ok(())
}

/// Whether the Android NDK can be found, as Nova looks for it. Without it the Android tests are
/// skipped, unless `SAG_REQUIRE_NDK=1` (set in CI) makes that a failure.
fn ndk_available() -> bool {
  let is_ndk = |dir: PathBuf| dir.join("toolchains").join("llvm").join("prebuilt").is_dir();
  let from_env = |name: &str| std::env::var_os(name).map(PathBuf::from).filter(|dir| !dir.as_os_str().is_empty());
  let direct = ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT"].iter().filter_map(|name| from_env(name)).any(is_ndk);
  let in_sdk = ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
    .iter()
    .filter_map(|name| from_env(name))
    .filter_map(|sdk| fs::read_dir(sdk.join("ndk")).ok())
    .any(|entries| entries.flatten().any(|entry| is_ndk(entry.path())));
  let found = direct || in_sdk;

  assert!(found || std::env::var("SAG_REQUIRE_NDK").map_or(true, |value| value != "1"), "SAG_REQUIRE_NDK=1 but the Android NDK was not found");

  if !found {
    eprintln!("skipped: the Android NDK was not found (set ANDROID_NDK_HOME)");
  }

  found
}

/// The ELF class (1 for 32 bits, 2 for 64) and machine of a file, or `None` if it is not ELF.
fn elf_kind(path: &Path) -> io::Result<Option<(u8, u16)>> {
  let bytes = fs::read(path)?;

  if !bytes.starts_with(b"\x7fELF") {
    return Ok(None);
  }

  // `e_machine` is little-endian on these targets.
  let class = bytes.get(4).copied();
  let machine = bytes.get(18..20).map(|pair| pair.iter().rev().fold(0u16, |value, byte| (value << 8) | u16::from(*byte)));

  Ok(class.zip(machine))
}

#[test]
fn cross_compiles_libraries_and_programs_for_android() -> io::Result<()> {
  if !ndk_available() {
    return Ok(());
  }

  let temp = TempDir::new()?;
  let geo = temp.path.join("geo");

  package(&geo, &format!("{}\n[link]\nsources = [\"c/helper.c\"]\n", manifest("geo", &[])), &[("lib.sag", LIBRARY_SOURCE)])?;
  fs::create_dir_all(geo.join("c"))?;
  fs::write(geo.join("c").join("helper.c"), "int helper_twice(int x) {\n  return 2 * x;\n}\n")?;

  // ELF machines: AArch64, ARM and x86-64.
  for (target, class, machine) in [("aarch64-linux-android", 2, 0xB7), ("armv7-linux-androideabi", 1, 0x28), ("x86_64-linux-android", 2, 0x3E)] {
    assert_success(&nova(&geo, &["build", "--target", target])?);

    let dir = geo.join("target").join(target).join("debug");

    assert_eq!(elf_kind(&dir.join("libgeo.so"))?, Some((class, machine)), "{target}");
    assert!(dir.join("libgeo.a").is_file() && dir.join("geo.h").is_file(), "{target}");
  }

  let app = temp.path.join("app");

  package(&app, &manifest("app", &[]), &[("main.sag", "fn main() {\n  println(\"hola\");\n}\n")])?;
  assert_success(&nova(&app, &["build", "--target", "aarch64-linux-android"])?);
  assert_eq!(elf_kind(&app.join("target").join("aarch64-linux-android").join("debug").join("app"))?, Some((2, 0xB7)));

  // It cannot run here, nor be sanitized.
  assert_eq!(nova(&app, &["run", "--target", "aarch64-linux-android"])?.status.code(), Some(1));
  assert_eq!(nova(&app, &["build", "--target", "aarch64-linux-android", "--sanitize"])?.status.code(), Some(1));

  Ok(())
}

#[test]
fn checks_integer_sizes_for_the_target() -> io::Result<()> {
  let temp = TempDir::new()?;
  let app = temp.path.join("app");

  // `usize` has 32 bits on `armv7`, and `c_long` 64 on 64-bit Android.
  package(
    &app,
    &manifest("app", &[]),
    &[(
      "main.sag",
      "use std::ffi::c_long;\n\nfn main() {\n  const big: usize = 5_000_000_000;\n  const long: c_long = 5_000_000_000;\n  println(`${big} ${long}`);\n}\n",
    )],
  )?;

  let armv7 = nova(&app, &["check", "--target", "armv7-linux-androideabi", "--diagnostic-format", "short"])?;

  assert!(stderr(&armv7).contains("main.sag:4:22: error[E0302]"), "{}", stderr(&armv7));
  assert!(stderr(&armv7).contains("main.sag:5:24: error[E0302]"), "{}", stderr(&armv7));
  assert_success(&nova(&app, &["check", "--target", "aarch64-linux-android"])?);

  // Unknown targets are reported with the supported ones.
  let unknown = nova(&app, &["check", "--target", "riscv64-linux-android"])?;

  assert_eq!(unknown.status.code(), Some(1));
  assert!(stderr(&unknown).contains("aarch64-linux-android"), "{}", stderr(&unknown));

  Ok(())
}

#[test]
fn builds_libraries_for_ios_on_macos_only() -> io::Result<()> {
  let temp = TempDir::new()?;
  let geo = temp.path.join("geo");

  package(&geo, &manifest("geo", &[]), &[("lib.sag", "pub extern \"C\" fn geo_one() -> i32 {\n  1\n}\n")])?;

  let app = temp.path.join("app");

  package(&app, &manifest("app", &[]), &[("main.sag", "fn main() {}\n")])?;

  // iOS has no standalone programs.
  assert_eq!(nova(&app, &["build", "--target", "aarch64-apple-ios"])?.status.code(), Some(1));

  let build = nova(&geo, &["build", "--target", "aarch64-apple-ios"])?;

  if !cfg!(target_os = "macos") {
    assert_eq!(build.status.code(), Some(1));
    assert!(stderr(&build).contains("only be built on macOS"), "{}", stderr(&build));

    return Ok(());
  }

  assert_success(&build);

  let device = geo.join("target").join("aarch64-apple-ios").join("debug");

  // Only the static library by default, which Xcode links.
  assert!(device.join("libgeo.a").is_file() && device.join("geo.h").is_file());
  assert!(!device.join("libgeo.dylib").exists());

  // The simulator, with the dynamic library too.
  fs::write(geo.join("Nova.toml"), format!("{}\n[library]\noutputs = [\"static\", \"dynamic\"]\n", manifest("geo", &[])))?;
  assert_success(&nova(&geo, &["build", "--target", "aarch64-apple-ios-sim"])?);
  assert!(geo.join("target").join("aarch64-apple-ios-sim").join("debug").join("libgeo.dylib").is_file());

  Ok(())
}

/// Copies the directory `from` into `to`, recursively.
fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
  fs::create_dir_all(to)?;

  for entry in fs::read_dir(from)? {
    let entry = entry?;
    let target = to.join(entry.file_name());

    if entry.file_type()?.is_dir() { copy_dir(&entry.path(), &target)? } else { fs::copy(entry.path(), target).map(|_| ())? }
  }

  Ok(())
}

/// The F3 acceptance program (docs/roadmap.md): `tests/acceptance/f3/` holds `wc`, a small `wc`
/// built as a package, which depends on the library package `textcount`, and a C program using
/// that library.
#[test]
fn f3_acceptance() -> io::Result<()> {
  let temp = TempDir::new()?;
  let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("tests").join("acceptance").join("f3");

  copy_dir(&root, &temp.path)?;

  let (wc, textcount) = (temp.path.join("wc"), temp.path.join("textcount"));

  fs::write(wc.join("a.txt"), "uno dos\ntres\n")?;
  fs::write(wc.join("b.txt"), "cuatro cinco seis\n")?;

  // Files, with sanitizers like the other end-to-end tests.
  let sanitize = std::env::var("SAG_TEST_SANITIZE").map_or(true, |value| value != "0");
  let mut args = vec!["run"];

  if sanitize {
    args.push("--sanitize");
  }

  let files = nova(&wc, &[args.as_slice(), &["--", "a.txt", "b.txt"]].concat())?;

  assert_success(&files);
  assert_eq!(stdout(&files), "      2       3      13 a.txt\n      1       3      18 b.txt\n      3       6      31 total\n");

  // Stdin.
  let mut child = Command::new(env!("CARGO_BIN_EXE_nova"))
    .args(&args)
    .current_dir(&wc)
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .spawn()?;

  if let Some(mut input) = child.stdin.take() {
    io::Write::write_all(&mut input, "hola mundo\nadiós\n".as_bytes())?;
  }

  let piped = child.wait_with_output()?;

  assert_success(&piped);
  assert_eq!(stdout(&piped), "      2       3      18\n");

  // A missing file is reported, and the others are still counted.
  let missing = nova(&wc, &[args.as_slice(), &["--", "a.txt", "missing.txt"]].concat())?;

  assert_eq!(missing.status.code(), Some(1), "{}", stderr(&missing));
  assert!(stderr(&missing).contains("wc: missing.txt:"), "{}", stderr(&missing));
  assert!(stdout(&missing).contains(" a.txt\n"), "{}", stdout(&missing));

  // The library, from C: linked statically and dynamically, compiled with warnings as errors.
  assert_success(&nova(&textcount, &["build"])?);

  let target = textcount.join("target").join("debug");
  let header = fs::read_to_string(target.join("textcount.h"))?;

  assert!(header.contains("uint64_t textcount_words(const char *text);"), "{header}");
  fs::copy(textcount.join("..").join("c").join("main.c"), target.join("main.c"))?;

  let compiler = c_compiler();
  let target_triple = c_target(&compiler)?;
  let windows = target_triple.contains("windows") || target_triple.contains("mingw");
  let mingw = target_triple.contains("windows-gnu") || target_triple.contains("mingw");
  let (static_lib, import_lib) = match (windows, mingw) {
    (true, false) => ("textcount.lib", "textcount.dll.lib"),
    (true, true) => ("libtextcount.a", "libtextcount.dll.a"),
    _ => ("libtextcount.a", ""),
  };
  let werror = if is_cl(&compiler) { "/WX" } else { "-Werror" };
  let libm: Vec<String> = if windows { Vec::new() } else { vec!["-lm".to_string()] };
  let dynamic: Vec<String> = if windows {
    vec![import_lib.to_string()]
  } else {
    vec!["-L.".to_string(), "-ltextcount".to_string(), format!("-Wl,-rpath,{}", target.display())]
  };

  for (name, libraries) in [("c_static", vec![static_lib.to_string()]), ("c_dynamic", dynamic)] {
    let executable = exe_name(name);
    let libraries: Vec<String> = [vec![werror.to_string()], libraries, libm.clone()].concat();
    let compile = compile_c_program(&compiler, &target, "main.c", &libraries, &executable)?;

    assert!(compile.status.success(), "{name}: {}", stderr(&compile));

    let run = Command::new(target.join(&executable)).current_dir(&target).output()?;

    assert_eq!(stdout(&run), "2 3\n", "{name}: {}", stderr(&run));
  }

  // Android: the tool and the library. iOS runs no standalone programs: the library is checked
  // for it everywhere, and built on macOS.
  if ndk_available() {
    for (target, machine) in [("aarch64-linux-android", 0xB7), ("armv7-linux-androideabi", 0x28), ("x86_64-linux-android", 0x3E)] {
      assert_success(&nova(&wc, &["build", "--target", target])?);
      assert_success(&nova(&textcount, &["build", "--target", target])?);
      assert_eq!(elf_kind(&wc.join("target").join(target).join("debug").join("wc"))?.map(|kind| kind.1), Some(machine), "{target}");
    }
  }

  assert_success(&nova(&textcount, &["check", "--target", "aarch64-apple-ios"])?);

  if cfg!(target_os = "macos") {
    assert_success(&nova(&textcount, &["build", "--target", "aarch64-apple-ios"])?);
    assert!(textcount.join("target").join("aarch64-apple-ios").join("debug").join("libtextcount.a").is_file());
  }

  Ok(())
}
