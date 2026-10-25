//! Compilation driver: runs the pipeline (docs/architecture.md#pipeline), prints diagnostics,
//! writes the generated C with the embedded runtime, and invokes the C compiler.

mod header;
mod targets;

pub use targets::{Platform, TARGETS, Target};

use std::collections::HashSet;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sagitario_ast::{Module, Package, PackageInfo, TargetInfo};
use sagitario_diagnostics::{DiagnosticFormat, Diagnostics, render};
use sagitario_span::SourceMap;

/// Exit code when compilation fails or the input cannot be read.
pub const EXIT_FAILURE: u8 = 1;
/// Exit code for an internal compiler error.
pub const EXIT_ICE: u8 = 101;

const RUNTIME_HEADER: &str = include_str!("../../../runtime/sagitario_runtime.h");
const RUNTIME_SOURCE: &str = include_str!("../../../runtime/sagitario_runtime.c");

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompileOptions {
  pub input: PathBuf,
  pub emit_c: bool,
  pub cc: Option<String>,
  pub diagnostic_format: DiagnosticFormat,
  /// Arguments forwarded to the compiled program (`sag run file.sag -- args`).
  pub program_args: Vec<String>,
  /// Release mode: integer overflow wraps and C is optimized (docs/spec/02-types.md#build-modes).
  pub release: bool,
  /// Compile with AddressSanitizer and UndefinedBehaviorSanitizer where the C compiler supports
  /// them (docs/testing.md#memory-checks).
  pub sanitize: bool,
  /// With `sanitize`: ThreadSanitizer instead, where the C compiler supports it
  /// (`--sanitize=thread`).
  pub thread_sanitizer: bool,
  /// Where `build` and `run` write the executable (and `--emit-c` the C); by default next to
  /// the input for `build`, and in a temporary directory for `run`.
  pub output: Option<PathBuf>,
  /// The input is the root of a library: it needs no `main` and is only checked.
  pub library: bool,
  /// Every package the input depends on, directly or not (docs/nova.md#dependencies).
  pub dependencies: Vec<Dependency>,
  /// Indices in `dependencies` of the input's own dependencies.
  pub direct_dependencies: Vec<usize>,
  /// The package's name, which names a library's files and its header.
  pub name: String,
  /// C libraries and sources to link (docs/nova.md#linking-c).
  pub link: Link,
  /// The name of the target to compile for (docs/nova.md#targets); the machine running the
  /// compiler when `None`.
  pub target: Option<String>,
  /// `sag check --dump=...`: print an intermediate result instead of checking.
  pub dump: Option<Dump>,
}

/// What `--dump` prints, for the differential tests of the self-hosted compiler
/// (docs/adr/0050-lexer-port-and-token-dumps.md). It is not part of the documented interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dump {
  /// The tokens of the input file.
  Tokens,
}

/// What `[link]` adds to a build: C libraries, where to find them, and C sources to compile.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Link {
  /// Names passed as `-lname`.
  pub libraries: Vec<String>,
  /// Directories passed as `-Ldir`.
  pub search_paths: Vec<PathBuf>,
  /// C files compiled as C11 and linked with the program or library.
  pub sources: Vec<PathBuf>,
}

impl Link {
  fn is_empty(&self) -> bool {
    self.libraries.is_empty() && self.search_paths.is_empty() && self.sources.is_empty()
  }
}

/// The files `build_library` writes, besides the header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryOutputs {
  pub dir: PathBuf,
  /// `libname.a`, or `name.lib` with MSVC.
  pub static_lib: bool,
  /// `libname.so`, `libname.dylib`, or `name.dll` with its import library.
  pub dynamic_lib: bool,
}

/// A library package the program depends on, compiled together with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
  /// The name its dependents use as a path root.
  pub name: String,
  /// Its root module, `src/lib.sag`.
  pub entry: PathBuf,
  /// Indices in `CompileOptions::dependencies` of its own dependencies.
  pub dependencies: Vec<usize>,
}

/// The C compiler flags of `--sanitize`: any undefined behavior aborts the program.
const SANITIZE_FLAGS: [&str; 2] = ["-fsanitize=address,undefined", "-fno-sanitize-recover=undefined"];

/// The C compiler flags of `--sanitize=thread`: a program with a data race exits with an error.
const THREAD_SANITIZE_FLAGS: [&str; 1] = ["-fsanitize=thread"];

/// The stack of the thread running the compiler. The passes recurse over the syntax tree, and
/// deeply nested code needs more than the main thread's stack (1 MiB on Windows), above all in
/// debug builds.
const STACK_SIZE: usize = 64 << 20;

/// Runs `work` (the whole command) on a thread with a large stack. A panic in it, which is an
/// internal compiler error already printed, gives the exit code 101.
pub fn on_large_stack(work: fn() -> std::process::ExitCode) -> std::process::ExitCode {
  match std::thread::Builder::new().stack_size(STACK_SIZE).spawn(work).map(std::thread::JoinHandle::join) {
    Ok(Ok(code)) => code,
    _ => std::process::ExitCode::from(EXIT_ICE),
  }
}

/// `sag check`: runs the front end only.
pub fn check(options: &CompileOptions) -> u8 {
  if options.dump == Some(Dump::Tokens) {
    return dump_tokens(options);
  }

  match front_end(options, false) {
    Ok(_) => 0,
    Err(code) => code,
  }
}

/// `sag build`: writes an executable next to the source file.
pub fn build(options: &CompileOptions) -> u8 {
  let executable = options.output.clone().unwrap_or_else(|| options.input.with_extension(executable_extension(options.target.as_deref())));
  match compile_to(options, &executable) {
    Ok(compiler) => {
      if let Some(dir) = compiler.sanitizer_runtime_dir() {
        eprintln!("note: the program needs the sanitizer runtime: add `{}` to PATH to run it", dir.display());
      }
      0
    }
    Err(code) => code,
  }
}

/// `sag run`: builds into a temporary directory (or `options.output`), runs the program and
/// returns its exit code.
pub fn run(options: &CompileOptions) -> u8 {
  if let Some(target) = &options.target {
    eprintln!("error: a program built for `{target}` cannot run on this machine");
    eprintln!("note: build it with `build`, then copy it to the device (for Android: `adb push` and `adb shell`)");
    return EXIT_FAILURE;
  }

  let dir = match TempDir::create() {
    Ok(dir) => dir,
    Err(error) => {
      eprintln!("error: cannot create a temporary directory: {error}");
      return EXIT_FAILURE;
    }
  };
  let stem = options.input.file_stem().unwrap_or_default();
  let executable = options.output.clone().unwrap_or_else(|| dir.path.join(stem).with_extension(env::consts::EXE_EXTENSION));
  let compiler = match compile_to(options, &executable) {
    Ok(compiler) => compiler,
    Err(code) => return code,
  };
  let mut command = Command::new(&executable);
  command.args(&options.program_args);
  // On Windows the sanitizer runtime is a DLL next to the compiler (docs/adr/0018-windows-c-toolchain.md).
  if let Some(dir) = compiler.sanitizer_runtime_dir() {
    let mut paths = vec![dir];
    paths.extend(env::var_os("PATH").map(|path| env::split_paths(&path).collect::<Vec<_>>()).unwrap_or_default());
    if let Ok(path) = env::join_paths(paths) {
      command.env("PATH", path);
    }
  }
  match command.status() {
    // Exit codes that do not fit in a byte (e.g. Windows exception codes) and signals become 1.
    Ok(status) => status.code().and_then(|code| u8::try_from(code).ok()).unwrap_or(EXIT_FAILURE),
    Err(error) => {
      eprintln!("error: cannot run `{}`: {error}", executable.display());
      EXIT_FAILURE
    }
  }
}

/// `sag check --dump=tokens`: the tokens of the input file alone, and its lexical errors
/// (docs/adr/0050-lexer-port-and-token-dumps.md).
fn dump_tokens(options: &CompileOptions) -> u8 {
  let input = &options.input;
  let text = match fs::read_to_string(input) {
    Ok(text) => text,
    Err(error) => {
      eprintln!("error: cannot read `{}`: {error}", input.display());
      return EXIT_FAILURE;
    }
  };
  let mut sources = SourceMap::new();
  let mut diagnostics = Diagnostics::new();
  let id = sources.add(input.clone(), text);
  let Some(source) = sources.get(id) else {
    eprintln!("internal compiler error: source file was not registered");
    return EXIT_ICE;
  };
  let tokens = sagitario_lexer::lex(id, source.text(), &mut diagnostics);

  print!("{}", sagitario_lexer::dump(&tokens));

  match report(&mut diagnostics, &sources, options) {
    Ok(()) => 0,
    Err(code) => code,
  }
}

struct FrontEnd {
  c_source: Option<sagitario_codegen_c::GeneratedC>,
  /// A library's C header.
  header: Option<String>,
  /// The symbols of a library's exported functions.
  exports: Vec<String>,
}

/// Lexes, parses, resolves and type checks; with `generate`, also lowers to SIR and emits C.
fn front_end(options: &CompileOptions, generate: bool) -> Result<FrontEnd, u8> {
  let input = &options.input;
  if input.extension().and_then(|extension| extension.to_str()) != Some("sag") {
    eprintln!("error: `{}` is not a `.sag` file", input.display());
    return Err(EXIT_FAILURE);
  }
  if !input.is_file() {
    eprintln!("error: cannot find `{}`", input.display());
    return Err(EXIT_FAILURE);
  }
  let mut sources = SourceMap::new();
  let mut diagnostics = Diagnostics::new();
  // The input's package comes first; dependency `i` is package `i + 1`.
  let mut packages = vec![LoadedPackage::new(input, "", &options.direct_dependencies)];

  packages.extend(options.dependencies.iter().map(|dependency| LoadedPackage::new(&dependency.entry, &dependency.name, &dependency.dependencies)));

  let mut loader = Loader { packages, next_id: 0, modules: Vec::new(), std: Vec::new() };

  for package in 0..loader.packages.len() {
    let entry = loader.packages.get(package).map(|loaded| loaded.entry.clone()).unwrap_or_default();

    loader.load(&entry, Vec::new(), package, &mut sources, &mut diagnostics)?;
  }

  // Each loaded module may refer to more modules; the ones that exist as files are loaded too,
  // and so are the standard library modules it names.
  let mut attempted = HashSet::new();
  let mut attempted_std = HashSet::new();
  let mut index = 0;

  while let Some(module) = loader.modules.get(index) {
    let package = module.package;
    let names = loader.dependency_names(package);
    let targets = if loader.std.contains(&index) { Vec::new() } else { sagitario_resolve::module_references(module, &names) };
    let std_names = sagitario_resolve::std_references(module);

    for target in targets {
      let target_package = match &target.dependency {
        None => Some(package),
        Some(name) => loader.dependency(package, name),
      };

      if let Some(target_package) = target_package
        && attempted.insert((target_package, target.path.clone()))
        && let Some(file) = loader.module_file(target_package, &target.path)
      {
        loader.load(&file, target.path, target_package, &mut sources, &mut diagnostics)?;
      }
    }

    for name in std_names {
      if attempted_std.insert(name.clone())
        && let Some((_, text)) = STD_SOURCES.iter().find(|(module, _)| *module == name)
      {
        loader.load_std(&name, text, &mut sources, &mut diagnostics)?;
      }
    }

    index += 1;
  }

  // Malformed tokens or an incomplete syntax tree would only cause confusing follow-up errors,
  // so lexical and syntax errors stop here. Name, type and flow errors are reported together.
  report(&mut diagnostics, &sources, options)?;

  let prelude = loader.load_prelude(&mut sources, &mut diagnostics)?;

  report(&mut diagnostics, &sources, options)?;

  let infos = loader.packages.iter().map(|loaded| PackageInfo { name: loaded.name.clone(), dependencies: loaded.dependencies.clone() }).collect();
  let target = match &options.target {
    Some(name) => targets::find(name)?.info,
    None => TargetInfo::host(),
  };
  let package = Package { modules: loader.modules, prelude: Some(prelude), std: loader.std, library: options.library, packages: infos, target };
  let resolution = sagitario_resolve::resolve(&package, &mut diagnostics);
  let typed = sagitario_types::check(&package, &resolution, &mut diagnostics);

  report(&mut diagnostics, &sources, options)?;

  let (header, exports) = if options.library && generate {
    let exports = header::exports(&package, &resolution, &typed).into_iter().map(|export| export.symbol).collect();

    (Some(header::generate(&options.name, &package, &resolution, &typed, &sources)), exports)
  } else {
    (None, Vec::new())
  };

  let program = sagitario_sir::lower(&package, &resolution, &typed, &sources, &mut diagnostics).map_err(|error| {
    eprintln!("internal compiler error: {}", error.0);
    EXIT_ICE
  })?;

  // Generic code may instantiate itself endlessly, found while lowering.
  report(&mut diagnostics, &sources, options)?;

  if !generate {
    return Ok(FrontEnd { c_source: None, header, exports });
  }
  Ok(FrontEnd { c_source: Some(sagitario_codegen_c::generate(&program)), header, exports })
}

/// The part of the prelude written in Sagitario.
const PRELUDE: &str = include_str!("prelude.sag");

/// The modules of the standard library written in Sagitario (docs/spec/11-standard-library.md),
/// loaded when a program names them.
/// The file of `std` itself: helpers shared by its modules, which programs cannot name.
const STD_ROOT: &str = include_str!("std/std.sag");

const STD_SOURCES: [(&str, &str); 8] = [
  ("env", include_str!("std/env.sag")),
  ("fs", include_str!("std/fs.sag")),
  ("io", include_str!("std/io.sag")),
  ("path", include_str!("std/path.sag")),
  ("process", include_str!("std/process.sag")),
  ("sync", include_str!("std/sync.sag")),
  ("thread", include_str!("std/thread.sag")),
  ("time", include_str!("std/time.sag")),
];

/// The integer types, each with a function `range_<type>` in the prelude.
const INT_TYPES: [&str; 10] = ["i8", "i16", "i32", "i64", "isize", "u8", "u16", "u32", "u64", "usize"];

/// `start..end` (or `..=`) outside of `for`, forwards or reversed, for integers of type `TYPE`:
/// the iterator a range is. Steps never overflow, so ranges reaching the type's limits work.
const RANGE_TEMPLATE: &str = "
fn range_TYPE(start: TYPE, end: TYPE, inclusive: bool, reverse: bool) -> Iter<TYPE> {
  const cursor = new Cell<TYPE>(if reverse { end } else { start });
  const done = new Cell<bool>(false);

  new Iter<TYPE>(() => {
    if done.value {
      return None;
    }

    const value = cursor.value;

    if reverse {
      if inclusive {
        if value < start {
          done.value = true;
          return None;
        }

        if value == start { done.value = true; } else { cursor.value = value - 1; }
        Some(value)
      } else {
        if value <= start {
          done.value = true;
          return None;
        }

        cursor.value = value - 1;
        Some(value - 1)
      }
    } else if inclusive {
      if value > end {
        done.value = true;
        return None;
      }

      if value == end { done.value = true; } else { cursor.value = value + 1; }
      Some(value)
    } else {
      if value >= end {
        done.value = true;
        return None;
      }

      cursor.value = value + 1;
      Some(value)
    }
  })
}
";

/// The prelude's source: `prelude.sag` followed by the range functions.
fn prelude_source() -> String {
  let mut source = PRELUDE.to_string();

  for ty in INT_TYPES {
    source.push_str(&RANGE_TEMPLATE.replace("TYPE", ty));
  }

  source
}

/// Reads, lexes and parses the modules of a program (docs/spec/09-modules.md): the entry file is
/// the root module `pkg`, and `pkg::net::http` is the file `net/http.sag` next to it.
/// A package whose modules are loaded: the input's, or a dependency.
struct LoadedPackage {
  name: String,
  /// The directory of its root module, which holds its other modules.
  root: PathBuf,
  entry: PathBuf,
  /// Indices in `Loader::packages` of its dependencies.
  dependencies: Vec<usize>,
}

impl LoadedPackage {
  /// Dependency `i` of `CompileOptions::dependencies` is package `i + 1`.
  fn new(entry: &Path, name: &str, dependencies: &[usize]) -> LoadedPackage {
    LoadedPackage {
      name: name.to_string(),
      root: entry.parent().map(Path::to_path_buf).unwrap_or_default(),
      entry: entry.to_path_buf(),
      dependencies: dependencies.iter().map(|index| index + 1).collect(),
    }
  }
}

struct Loader {
  packages: Vec<LoadedPackage>,
  /// Keeps node ids unique across modules.
  next_id: u32,
  modules: Vec<Module>,
  /// Indices in `modules` of the standard library modules.
  std: Vec<usize>,
}

impl Loader {
  /// Adds a module unless it has lexical errors, which are reported later with the others.
  fn load(&mut self, file: &Path, path: Vec<String>, package: usize, sources: &mut SourceMap, diagnostics: &mut Diagnostics) -> Result<(), u8> {
    let text = match fs::read_to_string(file) {
      Ok(text) => text,
      Err(error) => {
        eprintln!("error: cannot read `{}`: {error}", file.display());
        return Err(EXIT_FAILURE);
      }
    };
    if u32::try_from(text.len()).is_err() {
      eprintln!("error: `{}` is too large (the limit is 4 GiB)", file.display());
      return Err(EXIT_FAILURE);
    }

    let id = sources.add(file.to_path_buf(), text);
    let Some(source) = sources.get(id) else {
      eprintln!("internal compiler error: source file was not registered");
      return Err(EXIT_ICE);
    };
    let errors = diagnostics.error_count();
    let tokens = sagitario_lexer::lex(id, source.text(), diagnostics);

    if diagnostics.error_count() == errors {
      let mut module = sagitario_parser::parse_module_from(id, tokens, &mut self.next_id, diagnostics);

      module.path = path;
      module.package = package;
      module.location = Some(file.display().to_string());
      self.modules.push(module);
    }

    Ok(())
  }

  /// The names of the dependencies of `package`.
  fn dependency_names(&self, package: usize) -> Vec<String> {
    let dependencies = self.packages.get(package).map(|loaded| loaded.dependencies.as_slice()).unwrap_or_default();

    dependencies.iter().filter_map(|index| self.packages.get(*index)).map(|loaded| loaded.name.clone()).collect()
  }

  /// The package that `package` depends on under `name`.
  fn dependency(&self, package: usize, name: &str) -> Option<usize> {
    let dependencies = self.packages.get(package)?.dependencies.as_slice();

    dependencies.iter().copied().find(|index| self.packages.get(*index).is_some_and(|loaded| loaded.name == name))
  }

  /// Adds the prelude module, which every program sees, and returns its index.
  fn load_prelude(&mut self, sources: &mut SourceMap, diagnostics: &mut Diagnostics) -> Result<usize, u8> {
    self.load_embedded("<prelude>", prelude_source(), vec!["std".to_string(), "prelude".to_string()], sources, diagnostics)
  }

  /// Adds the standard library module `std::name`.
  /// The first one comes with the file of `std` itself.
  fn load_std(&mut self, name: &str, text: &str, sources: &mut SourceMap, diagnostics: &mut Diagnostics) -> Result<(), u8> {
    if self.std.is_empty() {
      let index = self.load_embedded("<std>", STD_ROOT.to_string(), vec!["std".to_string()], sources, diagnostics)?;

      self.std.push(index);
    }

    let path = vec!["std".to_string(), name.to_string()];
    let index = self.load_embedded(&format!("<std::{name}>"), text.to_string(), path, sources, diagnostics)?;

    self.std.push(index);

    Ok(())
  }

  /// Adds a module whose source the compiler embeds.
  fn load_embedded(
    &mut self,
    file: &str,
    text: String,
    path: Vec<String>,
    sources: &mut SourceMap,
    diagnostics: &mut Diagnostics,
  ) -> Result<usize, u8> {
    let id = sources.add(PathBuf::from(file), text);
    let Some(source) = sources.get(id) else {
      eprintln!("internal compiler error: `{file}` was not registered");
      return Err(EXIT_ICE);
    };
    let tokens = sagitario_lexer::lex(id, source.text(), diagnostics);
    let mut module = sagitario_parser::parse_module_from(id, tokens, &mut self.next_id, diagnostics);

    module.path = path;
    self.modules.push(module);

    Ok(self.modules.len() - 1)
  }

  /// The file of the module at `path` in `package`, if it exists with exactly these names (also
  /// on case-insensitive file systems) and is not the package's entry file.
  fn module_file(&self, package: usize, path: &[String]) -> Option<PathBuf> {
    let package = self.packages.get(package)?;
    let (last, dirs) = path.split_last()?;
    let mut dir = package.root.clone();

    for name in dirs {
      if !has_entry(&dir, name, true) {
        return None;
      }

      dir.push(name);
    }

    let name = format!("{last}.sag");

    if !has_entry(&dir, &name, false) {
      return None;
    }

    let file = dir.join(name);
    let same = |a: &Path, b: &Path| matches!((fs::canonicalize(a), fs::canonicalize(b)), (Ok(a), Ok(b)) if a == b);

    if same(&file, &package.entry) { None } else { Some(file) }
  }
}

/// Whether `dir` has an entry with exactly this name that is a directory (or a file).
fn has_entry(dir: &Path, name: &str, directory: bool) -> bool {
  let dir = if dir.as_os_str().is_empty() { Path::new(".") } else { dir };
  let Ok(entries) = fs::read_dir(dir) else { return false };

  entries.flatten().any(|entry| entry.file_name().to_str() == Some(name) && entry.file_type().is_ok_and(|kind| kind.is_dir() == directory))
}

/// Prints the collected diagnostics in source order and stops the pipeline if any of them is
/// an error.
fn report(diagnostics: &mut Diagnostics, sources: &SourceMap, options: &CompileOptions) -> Result<(), u8> {
  let errors = diagnostics.error_count();
  let warnings = diagnostics.warning_count();
  let color = use_color(options.diagnostic_format);

  for diagnostic in diagnostics.take() {
    eprintln!("{}", render(&diagnostic, sources, options.diagnostic_format, color));
  }

  if options.diagnostic_format == DiagnosticFormat::Human {
    let input = options.input.display();
    let warnings_text = format!("{warnings} warning{}", plural(warnings));

    if errors > 0 {
      let also = if warnings > 0 { format!("; {warnings_text} emitted") } else { String::new() };

      eprintln!("error: could not compile `{input}` due to {errors} previous error{}{also}", plural(errors));
    } else if warnings > 0 {
      eprintln!("warning: `{input}` generated {warnings_text}");
    }
  }

  if errors > 0 { Err(EXIT_FAILURE) } else { Ok(()) }
}

fn plural(count: usize) -> &'static str {
  if count == 1 { "" } else { "s" }
}

/// Colors only the human format on an interactive stderr, honoring `NO_COLOR` and
/// `CLICOLOR_FORCE`. Classic Windows consoles do not interpret ANSI codes unless asked through
/// the Windows API (which would need `unsafe`), so on Windows colors are used only in terminals
/// known to support them.
pub fn use_color(format: DiagnosticFormat) -> bool {
  use std::io::IsTerminal;

  let set = |name: &str| env::var_os(name).is_some_and(|value| !value.is_empty());

  if format != DiagnosticFormat::Human || set("NO_COLOR") {
    return false;
  }

  if env::var("CLICOLOR_FORCE").is_ok_and(|value| value != "0") {
    return true;
  }

  let ansi_terminal = !cfg!(windows) || ["WT_SESSION", "TERM", "TERM_PROGRAM", "ConEmuANSI"].into_iter().any(set);

  ansi_terminal && std::io::stderr().is_terminal()
}

/// What failed while building with the C toolchain.
enum Failure {
  /// The generated C, which is a compiler bug.
  Generated(String),
  /// A C source file of the package.
  Source(PathBuf, String),
  /// Linking, or archiving a static library.
  Link(String),
  /// A tool that could not run.
  Spawn(String, io::Error),
}

/// Runs the C compiler and the archiver on the files of a build.
struct Toolchain<'a> {
  compiler: &'a CCompiler,
  /// The temporary directory holding the generated C and the objects.
  dir: &'a Path,
  mode: &'a [&'a str],
  sanitize: &'a [&'a str],
  /// Position-independent code, for dynamic libraries outside Windows.
  pic: bool,
  /// Whether the program uses threads (`GeneratedC::threads`).
  threads: bool,
}

impl Toolchain<'_> {
  fn output(&self, command: &mut Command, tool: &str) -> Result<std::process::Output, Failure> {
    command.output().map_err(|error| Failure::Spawn(tool.to_string(), error))
  }

  /// What a failed tool printed: `cl` and `lib` print their errors to stdout.
  fn failure_text(&self, output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if self.compiler.cl { format!("{}{stderr}", String::from_utf8_lossy(&output.stdout)) } else { stderr }
  }

  /// Compiles one C file to an object; the generated C must be strict C11.
  fn compile(&self, source: &Path, object: &Path, generated: bool) -> Result<(), Failure> {
    let mut command = self.compiler.command();

    if self.compiler.cl {
      // MSVC has no option rejecting extensions in C; `/W3` keeps its warnings to likely bugs.
      command.args(["/nologo", "/std:c11", "/W3", "/utf-8"]).args(self.mode);

      // MSVC provides `<stdatomic.h>` only behind this option (docs/adr/0045-threads-and-send.md).
      if self.threads {
        command.arg("/experimental:c11atomics");
      }

      command.arg("/I").arg(self.dir).arg("/c").arg(source).arg(format!("/Fo{}", object.display()));
    } else {
      command.arg("-std=c11");

      if generated {
        command.arg("-pedantic-errors");
      }

      command.args(self.mode).args(self.sanitize);

      if self.pic {
        command.arg("-fPIC");
      }

      command.args(self.pthread());
      command.arg("-I").arg(self.dir).arg("-c").arg(source).arg("-o").arg(object);
    }

    let output = self.output(&mut command, &self.compiler.command)?;

    if output.status.success() {
      return Ok(());
    }

    let stderr = self.failure_text(&output);

    Err(if generated { Failure::Generated(stderr) } else { Failure::Source(source.to_path_buf(), stderr) })
  }

  /// The objects of the generated C files and of the package's own C sources.
  fn objects(&self, generated: &[PathBuf], sources: &[PathBuf]) -> Result<Vec<PathBuf>, Failure> {
    let mut objects = Vec::new();

    for (index, source) in generated.iter().chain(sources).enumerate() {
      let object = self.dir.join(format!("object{index}.{}", if self.compiler.cl { "obj" } else { "o" }));

      self.compile(source, &object, index < generated.len())?;
      objects.push(object);
    }

    Ok(objects)
  }

  /// The libraries to link: the package's, then libm outside Windows (`fmod` and friends). For
  /// `cl` they are options of the linker, after `/link`.
  fn link_args(&self, link: &Link) -> Vec<String> {
    let mut args = Vec::new();

    if self.compiler.cl {
      for path in &link.search_paths {
        args.push(format!("/LIBPATH:{}", path.display()));
      }

      for library in &link.libraries {
        args.push(format!("{library}.lib"));
      }

      return args;
    }

    for path in &link.search_paths {
      args.push(format!("-L{}", path.display()));
    }

    for library in &link.libraries {
      args.push(format!("-l{library}"));
    }

    if !self.compiler.windows() {
      args.push("-lm".to_string());
    }

    args.extend(self.pthread().iter().map(|arg| arg.to_string()));
    args
  }

  /// POSIX threads, for programs using threads outside Windows (which uses its own API).
  fn pthread(&self) -> &'static [&'static str] {
    if self.threads && !self.compiler.windows() { &["-pthread"] } else { &[] }
  }

  fn link(&self, command: &mut Command) -> Result<(), Failure> {
    let output = self.output(command, &self.compiler.command)?;

    if output.status.success() { Ok(()) } else { Err(Failure::Link(self.failure_text(&output))) }
  }

  fn executable(&self, objects: &[PathBuf], link: &Link, executable: &Path) -> Result<(), Failure> {
    let mut command = self.compiler.command();

    if self.compiler.cl {
      command.arg("/nologo").args(self.mode).args(objects).arg(format!("/Fe{}", executable.display())).arg("/link").args(self.link_args(link));
    } else {
      command.args(self.mode).args(self.sanitize).args(objects).arg("-o").arg(executable).args(self.link_args(link));
    }

    self.link(&mut command)
  }
}

/// Writes the generated C, with the runtime, into `dir`, and returns the files to compile.
fn write_generated(generated: &sagitario_codegen_c::GeneratedC, dir: &Path) -> Result<Vec<PathBuf>, u8> {
  let program_c = dir.join("program.c");
  let runtime_c = dir.join("sagitario_runtime.c");
  let ffi_c = dir.join("sag_ffi.c");
  let runtime = if generated.threads { format!("#define SAG_THREADS 1\n{RUNTIME_SOURCE}") } else { RUNTIME_SOURCE.to_string() };
  let written = fs::write(&program_c, &generated.program)
    .and_then(|()| fs::write(dir.join(sagitario_codegen_c::RUNTIME_HEADER), RUNTIME_HEADER))
    .and_then(|()| fs::write(&runtime_c, runtime))
    .and_then(|()| match &generated.ffi {
      Some(ffi) => fs::write(&ffi_c, ffi),
      None => Ok(()),
    });

  if let Err(error) = written {
    eprintln!("error: cannot write generated C to `{}`: {error}", dir.display());
    return Err(EXIT_FAILURE);
  }

  let mut files = vec![program_c, runtime_c];

  if generated.ffi.is_some() {
    files.push(ffi_c);
  }

  Ok(files)
}

/// Writes the generated C next to `base` for `--emit-c`.
fn emit_c(generated: &sagitario_codegen_c::GeneratedC, base: &Path) -> Result<(), u8> {
  let c_path = base.with_extension("c");
  let ffi_path = c_path.with_file_name(format!("{}_ffi.c", base.file_stem().unwrap_or_default().to_string_lossy()));
  let written = fs::write(&c_path, &generated.program).and_then(|()| match &generated.ffi {
    Some(ffi) => fs::write(&ffi_path, ffi),
    None => Ok(()),
  });

  if let Err(error) = written {
    eprintln!("error: cannot write the generated C next to `{}`: {error}", base.display());
    return Err(EXIT_FAILURE);
  }

  Ok(())
}

/// Reports a failed build. Failures with the package's own C sources or libraries are errors in
/// the package; failures with the generated C alone are internal compiler errors.
fn report_failure(failure: Failure, compiler: &CCompiler, link: &Link) -> u8 {
  match failure {
    Failure::Generated(stderr) => {
      eprintln!("internal compiler error: the generated C failed to compile with `{}`", compiler.command);
      eprintln!("{stderr}");
      EXIT_ICE
    }
    Failure::Source(path, stderr) => {
      eprintln!("error: cannot compile the C source `{}`", path.display());
      eprintln!("{stderr}");
      EXIT_FAILURE
    }
    Failure::Link(stderr) if link.is_empty() => {
      eprintln!("internal compiler error: the generated C failed to link with `{}`", compiler.command);
      eprintln!("{stderr}");
      EXIT_ICE
    }
    Failure::Link(stderr) => {
      eprintln!("error: linking failed; check the C libraries and sources of `[link]`");
      eprintln!("{stderr}");
      EXIT_FAILURE
    }
    Failure::Spawn(tool, error) => {
      eprintln!("error: cannot run `{tool}`: {error}");
      EXIT_FAILURE
    }
  }
}

/// Compiles the program into `executable` and returns the C compiler used.
fn compile_to(options: &CompileOptions, executable: &Path) -> Result<CCompiler, u8> {
  if options.library {
    eprintln!("error: `{}` is the root of a library, which has no executable", options.input.display());
    return Err(EXIT_FAILURE);
  }

  let Some(generated) = front_end(options, true)?.c_source else {
    eprintln!("internal compiler error: no C was generated");
    return Err(EXIT_ICE);
  };

  if options.emit_c {
    emit_c(&generated, options.output.as_ref().unwrap_or(&options.input))?;
  }

  let mut compiler = compiler_for(options)?;

  if options.target.is_some() && options.sanitize {
    eprintln!("error: `--sanitize` only works when compiling for this machine");
    return Err(EXIT_FAILURE);
  }

  if let Some(target) = options.target.as_deref().map(targets::find).transpose()?
    && target.platform == Platform::Ios
  {
    eprintln!("error: iOS runs no standalone programs; build a library for `{}` with Nova and add it to an Xcode project", target.name);
    return Err(EXIT_FAILURE);
  }

  compiler.sanitized = options.sanitize && compiler.supports_sanitizers();

  if options.sanitize && compiler.cl {
    eprintln!("warning: `cl` builds without sanitizers: MSVC has no UndefinedBehaviorSanitizer (docs/adr/0044-c-toolchains.md)");
  }

  let thread_sanitizer = compiler.sanitized && options.thread_sanitizer && compiler.supports_thread_sanitizer();

  if compiler.sanitized && options.thread_sanitizer && !thread_sanitizer {
    eprintln!("warning: `{}` has no ThreadSanitizer for `{}`; building with ASan and UBSan instead", compiler.command, compiler.target);
  }

  let sanitize_flags: &[&str] = if thread_sanitizer { &THREAD_SANITIZE_FLAGS } else { &SANITIZE_FLAGS };

  let dir = TempDir::create().map_err(|error| {
    eprintln!("error: cannot create a temporary directory: {error}");
    EXIT_FAILURE
  })?;
  let files = write_generated(&generated, &dir.path)?;
  let mode = compiler.mode(options.release);
  let build = |sanitize: &[&str]| {
    let toolchain = Toolchain { compiler: &compiler, dir: &dir.path, mode, sanitize, pic: false, threads: generated.threads };
    let objects = toolchain.objects(&files, &options.link.sources)?;

    toolchain.executable(&objects, &options.link, executable)
  };

  match build(if compiler.sanitized { sanitize_flags } else { &[] }) {
    Ok(()) => Ok(compiler),
    // It builds without sanitizers: the toolchain lacks their libraries.
    Err(Failure::Generated(stderr) | Failure::Link(stderr)) if compiler.sanitized && build(&[]).is_ok() => {
      eprintln!("error: the C compiler `{}` cannot build with sanitizers (`{}`)", compiler.command, sanitize_flags.join(" "));
      eprintln!("{stderr}");
      eprintln!("note: install its sanitizer libraries, or build without `--sanitize`");
      Err(EXIT_FAILURE)
    }
    Err(failure) => Err(report_failure(failure, &compiler, &options.link)),
  }
}

/// The files `build_library` writes.
fn library_files(compiler: &CCompiler, outputs: &LibraryOutputs, name: &str) -> (Option<PathBuf>, Option<(PathBuf, Option<PathBuf>)>) {
  let (windows, mingw, apple) = (compiler.windows(), compiler.mingw(), compiler.apple());
  let static_name = if windows && !mingw { format!("{name}.lib") } else { format!("lib{name}.a") };
  let dynamic = if windows {
    let import = if mingw { format!("lib{name}.dll.a") } else { format!("{name}.dll.lib") };

    (outputs.dir.join(format!("{name}.dll")), Some(outputs.dir.join(import)))
  } else if apple {
    (outputs.dir.join(format!("lib{name}.dylib")), None)
  } else {
    (outputs.dir.join(format!("lib{name}.so")), None)
  };

  (outputs.static_lib.then(|| outputs.dir.join(static_name)), outputs.dynamic_lib.then_some(dynamic))
}

/// The archiver for static libraries: `SAG_AR`, then `llvm-ar` next to the C compiler, then
/// `llvm-ar` and `ar` on `PATH`.
fn find_archiver(compiler: &CCompiler) -> Result<String, u8> {
  // MSVC's own librarian, next to `cl` (`lib.exe`).
  if compiler.cl {
    return Ok(Path::new(&compiler.command).with_file_name("lib").display().to_string());
  }

  if let Ok(archiver) = env::var("SAG_AR")
    && !archiver.trim().is_empty()
  {
    return Ok(archiver);
  }

  let mut candidates = Vec::new();

  if let Some(dir) = Path::new(&compiler.command).parent().filter(|dir| !dir.as_os_str().is_empty()) {
    candidates.push(dir.join("llvm-ar").with_extension(env::consts::EXE_EXTENSION).display().to_string());
  }

  candidates.push("llvm-ar".to_string());
  candidates.push("ar".to_string());

  // Only whether it starts: the `ar` of macOS has no `--version` and exits with an error.
  for candidate in candidates {
    if Command::new(&candidate).arg("--version").stdin(Stdio::null()).output().is_ok() {
      return Ok(candidate);
    }
  }

  eprintln!("error: no archiver found to build a static library; Sagitario needs `llvm-ar` or `ar`");
  eprintln!("note: install one next to the C compiler or on PATH, or set SAG_AR");
  Err(EXIT_FAILURE)
}

/// `nova build` of a library: its static and dynamic libraries and its header in
/// `outputs.dir`. Returns the files written.
pub fn build_library(options: &CompileOptions, outputs: &LibraryOutputs) -> Result<Vec<PathBuf>, u8> {
  if options.sanitize {
    eprintln!("error: libraries cannot be built with `--sanitize`: the programs using them would need the sanitizer runtime");
    return Err(EXIT_FAILURE);
  }

  let front = front_end(options, true)?;
  let (Some(generated), Some(header)) = (front.c_source, front.header) else {
    eprintln!("internal compiler error: no C was generated for the library");
    return Err(EXIT_ICE);
  };
  let name = &options.name;

  if front.exports.is_empty() {
    eprintln!("warning: the library `{name}` exports no functions; C can call only the functions declared `extern \"C\" fn`");
  }

  let compiler = compiler_for(options)?;
  let (static_lib, dynamic_lib) = library_files(&compiler, outputs, name);
  let header_path = outputs.dir.join(format!("{name}.h"));
  let mut written = Vec::new();

  if options.emit_c {
    emit_c(&generated, &outputs.dir.join(name))?;
  }

  let dir = TempDir::create().map_err(|error| {
    eprintln!("error: cannot create a temporary directory: {error}");
    EXIT_FAILURE
  })?;
  let files = write_generated(&generated, &dir.path)?;
  let mode = compiler.mode(options.release);
  let windows = compiler.windows();
  let report = |failure| report_failure(failure, &compiler, &options.link);

  if let Some(path) = static_lib {
    let archiver = find_archiver(&compiler)?;
    let toolchain = Toolchain { compiler: &compiler, dir: &dir.path, mode, sanitize: &[], pic: false, threads: generated.threads };
    let objects = toolchain.objects(&files, &options.link.sources).map_err(report)?;
    // `ar` adds to an existing archive, which could keep stale objects.
    let _ = fs::remove_file(&path);
    let mut command = Command::new(&archiver);

    if compiler.cl {
      command.arg("/NOLOGO").arg(format!("/OUT:{}", path.display())).args(&objects);
    } else {
      command.arg("rcs").arg(&path).args(&objects);
    }

    let output = toolchain.output(&mut command, &archiver).map_err(report)?;

    if !output.status.success() {
      return Err(report(Failure::Link(toolchain.failure_text(&output))));
    }

    written.push(path);
  }

  if let Some((path, import)) = dynamic_lib {
    let toolchain = Toolchain { compiler: &compiler, dir: &dir.path, mode, sanitize: &[], pic: !windows, threads: generated.threads };
    let objects = toolchain.objects(&files, &options.link.sources).map_err(report)?;
    let mut command = compiler.command();

    if compiler.cl {
      command.arg("/nologo").args(mode).arg("/LD").args(&objects).arg(format!("/Fe{}", path.display())).arg("/link");
    } else {
      command.args(mode).args(&objects).arg("-o").arg(&path);
    }

    export_list(&mut command, &compiler, &dir.path, &path, import.as_deref(), &front.exports)?;
    command.args(toolchain.link_args(&options.link));
    toolchain.link(&mut command).map_err(report)?;

    // The MSVC linker also writes an exports file next to the import library.
    if let Some(import) = import.as_deref().filter(|_| compiler.cl) {
      let _ = fs::remove_file(import.with_extension("exp"));
    }

    written.push(path);
    written.extend(import);
  }

  if let Err(error) = fs::write(&header_path, header) {
    eprintln!("error: cannot write `{}`: {error}", header_path.display());
    return Err(EXIT_FAILURE);
  }

  written.push(header_path);

  Ok(written)
}

/// Makes a dynamic library export only `exports`, with the linker's own means so that the C
/// stays strict C11: a module-definition file on Windows, a version script on Linux and an
/// exported symbols list on macOS.
fn export_list(command: &mut Command, compiler: &CCompiler, dir: &Path, library: &Path, import: Option<&Path>, exports: &[String]) -> Result<(), u8> {
  let (windows, mingw, apple) = (compiler.windows(), compiler.mingw(), compiler.apple());
  let (file, text) = if windows {
    let name = library.file_stem().unwrap_or_default().to_string_lossy();
    let symbols: String = exports.iter().map(|symbol| format!("  {symbol}\n")).collect();

    (dir.join("exports.def"), format!("LIBRARY {name}\nEXPORTS\n{symbols}"))
  } else if apple {
    (dir.join("exports.txt"), exports.iter().map(|symbol| format!("_{symbol}\n")).collect())
  } else {
    let symbols: String = exports.iter().map(|symbol| format!(" {symbol};")).collect();
    let global = if symbols.is_empty() { String::new() } else { format!("global:{symbols} ") };

    (dir.join("exports.map"), format!("{{ {global}local: *; }};\n"))
  };

  if let Err(error) = fs::write(&file, text) {
    eprintln!("error: cannot write `{}`: {error}", file.display());
    return Err(EXIT_FAILURE);
  }

  if compiler.cl {
    command.arg(format!("/DEF:{}", file.display()));

    if let Some(import) = import {
      command.arg(format!("/IMPLIB:{}", import.display()));
    }

    return Ok(());
  }

  command.arg("-shared");

  if mingw {
    command.arg(&file);

    if let Some(import) = import {
      command.arg(format!("-Wl,--out-implib,{}", import.display()));
    }
  } else if windows {
    command.arg(format!("-Wl,/DEF:{}", file.display()));

    if let Some(import) = import {
      command.arg(format!("-Wl,/IMPLIB:{}", import.display()));
    }
  } else if apple {
    let name = library.file_name().unwrap_or_default().to_string_lossy();

    command.arg("-dynamiclib").arg(format!("-Wl,-exported_symbols_list,{}", file.display())).arg(format!("-Wl,-install_name,@rpath/{name}"));
  } else {
    let name = library.file_name().unwrap_or_default().to_string_lossy();

    command.arg("-fPIC").arg(format!("-Wl,--version-script={}", file.display())).arg(format!("-Wl,-soname,{name}"));
  }

  Ok(())
}

/// The extension of executables for `target`: none on Android, `exe` on Windows.
pub fn executable_extension(target: Option<&str>) -> &'static str {
  if target.is_some() { "" } else { env::consts::EXE_EXTENSION }
}

/// The C compiler of the target of `options`, or of this machine.
fn compiler_for(options: &CompileOptions) -> Result<CCompiler, u8> {
  match &options.target {
    Some(name) => targets::find(name)?.compiler(),
    None => find_c_compiler(options.cc.as_deref()),
  }
}

struct CCompiler {
  command: String,
  /// Arguments every invocation needs, such as `--target=` when cross-compiling.
  args: Vec<String>,
  /// Target triple reported by `-dumpmachine`.
  target: String,
  /// Whether the program was compiled with sanitizers.
  sanitized: bool,
  /// MSVC's `cl`, whose options differ from clang's and gcc's (docs/adr/0044-c-toolchains.md).
  cl: bool,
}

impl CCompiler {
  /// A command running the compiler, with the arguments that select the target.
  fn command(&self) -> Command {
    let mut command = Command::new(&self.command);

    command.args(&self.args);
    command
  }

  /// Windows, with the MSVC or the MinGW toolchain (`x86_64-w64-mingw32`).
  fn windows(&self) -> bool {
    self.target.contains("windows") || self.mingw()
  }

  fn mingw(&self) -> bool {
    self.target.contains("windows-gnu") || self.target.contains("mingw")
  }

  fn apple(&self) -> bool {
    self.target.contains("apple") || self.target.contains("darwin")
  }

  /// MinGW toolchains ship no sanitizer runtime (docs/adr/0018-windows-c-toolchain.md), and MSVC
  /// has no UndefinedBehaviorSanitizer.
  fn supports_sanitizers(&self) -> bool {
    !self.mingw() && !self.cl
  }

  /// ThreadSanitizer exists for Linux and macOS, not for Windows.
  fn supports_thread_sanitizer(&self) -> bool {
    self.supports_sanitizers() && !self.windows()
  }

  /// The options of a debug or release build.
  fn mode(&self, release: bool) -> &'static [&'static str] {
    match (self.cl, release) {
      (true, true) => &["/O2", "/DSAG_RELEASE"],
      (true, false) => &["/Od", "/Z7"],
      (false, true) => &["-O2", "-DSAG_RELEASE"],
      (false, false) => &["-O0", "-g"],
    }
  }

  /// The directory of the sanitizer runtime DLLs, which a sanitized program needs on `PATH` on
  /// Windows; `None` elsewhere or when the program is not sanitized.
  fn sanitizer_runtime_dir(&self) -> Option<PathBuf> {
    if !self.sanitized || !self.windows() {
      return None;
    }
    let output = Command::new(&self.command).arg("-print-resource-dir").stdin(Stdio::null()).output().ok()?;
    let resource_dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (output.status.success() && !resource_dir.is_empty()).then(|| Path::new(&resource_dir).join("lib").join("windows"))
  }
}

/// Finds the C compiler: `--cc`, then `SAG_CC`, then `clang`, then `gcc` (docs/architecture.md).
fn find_c_compiler(explicit: Option<&str>) -> Result<CCompiler, u8> {
  let configured = explicit.map(str::to_string).or_else(|| env::var("SAG_CC").ok().filter(|value| !value.trim().is_empty()));
  let candidates = match &configured {
    Some(command) => vec![command.clone()],
    None => vec!["clang".to_string(), "gcc".to_string()],
  };
  for command in candidates {
    if is_cl(&command) {
      return msvc_compiler(command);
    }

    let Ok(output) = Command::new(&command).arg("-dumpmachine").stdin(Stdio::null()).output() else {
      continue;
    };
    if !output.status.success() {
      continue;
    }
    let target = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if cfg!(windows) && target.contains("windows-gnu") {
      eprintln!(
        "warning: `{command}` targets `{target}` (MinGW), which is supported best-effort and without sanitizers; \
         set SAG_CC to an LLVM clang targeting `x86_64-pc-windows-msvc` (docs/adr/0018-windows-c-toolchain.md)"
      );
    }
    return Ok(CCompiler { command, args: Vec::new(), target, sanitized: false, cl: false });
  }
  match configured {
    Some(command) => eprintln!("error: cannot run the C compiler `{command}` (from --cc or SAG_CC)"),
    None => eprintln!("error: no C compiler found; Sagitario needs `clang` (or `gcc`) on PATH\n{}", install_hint()),
  }
  Err(EXIT_FAILURE)
}

/// Whether `command` is MSVC's `cl`, which has no `-dumpmachine` and is recognized by its name.
fn is_cl(command: &str) -> bool {
  Path::new(command).file_stem().is_some_and(|stem| stem.eq_ignore_ascii_case("cl"))
}

/// MSVC's `cl`, which finds its headers and libraries through the variables a Developer Command
/// Prompt sets (`INCLUDE`, `LIB`).
fn msvc_compiler(command: String) -> Result<CCompiler, u8> {
  if env::var_os("INCLUDE").is_none_or(|include| include.is_empty()) {
    eprintln!("error: `{command}` needs the environment of a Developer Command Prompt for Visual Studio (`INCLUDE` is not set)");
    eprintln!("note: run `vcvars64.bat` from the Visual Studio installation first, or open a Developer Command Prompt");
    return Err(EXIT_FAILURE);
  }

  if Command::new(&command).stdin(Stdio::null()).output().is_err() {
    eprintln!("error: cannot run the C compiler `{command}` (from --cc or SAG_CC)");
    return Err(EXIT_FAILURE);
  }

  Ok(CCompiler { command, args: Vec::new(), target: format!("{}-pc-windows-msvc", env::consts::ARCH), sanitized: false, cl: true })
}

fn install_hint() -> &'static str {
  if cfg!(windows) {
    "  install LLVM (https://github.com/llvm/llvm-project/releases) and Visual Studio Build Tools, or set SAG_CC"
  } else if cfg!(target_os = "macos") {
    "  install the Xcode command line tools: `xcode-select --install`, or set SAG_CC"
  } else {
    "  install clang with your package manager (e.g. `sudo apt install clang`), or set SAG_CC"
  }
}

/// A uniquely named temporary directory, removed on drop.
struct TempDir {
  path: PathBuf,
}

impl TempDir {
  fn create() -> io::Result<TempDir> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_nanos()).unwrap_or(0);
    let path = env::temp_dir().join(format!("sag-{}-{nanos}-{count}", std::process::id()));
    fs::create_dir_all(&path)?;
    Ok(TempDir { path })
  }
}

impl Drop for TempDir {
  fn drop(&mut self) {
    // Best effort: a leftover temporary directory is harmless.
    let _ = fs::remove_dir_all(&self.path);
  }
}
