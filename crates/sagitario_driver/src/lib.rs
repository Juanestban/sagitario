//! Compilation driver: runs the pipeline (docs/architecture.md#pipeline), prints diagnostics,
//! writes the generated C with the embedded runtime, and invokes the C compiler.

use std::collections::HashSet;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sagitario_ast::{Module, Package, PackageInfo};
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
  /// Where `build` and `run` write the executable (and `--emit-c` the C); by default next to
  /// the input for `build`, and in a temporary directory for `run`.
  pub output: Option<PathBuf>,
  /// The input is the root of a library: it needs no `main` and is only checked.
  pub library: bool,
  /// Every package the input depends on, directly or not (docs/nova.md#dependencies).
  pub dependencies: Vec<Dependency>,
  /// Indices in `dependencies` of the input's own dependencies.
  pub direct_dependencies: Vec<usize>,
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

/// `sag check`: runs the front end only.
pub fn check(options: &CompileOptions) -> u8 {
  match front_end(options, false) {
    Ok(_) => 0,
    Err(code) => code,
  }
}

/// `sag build`: writes an executable next to the source file.
pub fn build(options: &CompileOptions) -> u8 {
  let executable = options.output.clone().unwrap_or_else(|| options.input.with_extension(env::consts::EXE_EXTENSION));
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

struct FrontEnd {
  c_source: Option<sagitario_codegen_c::GeneratedC>,
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
  let package = Package { modules: loader.modules, prelude: Some(prelude), std: loader.std, library: options.library, packages: infos };
  let resolution = sagitario_resolve::resolve(&package, &mut diagnostics);
  let typed = sagitario_types::check(&package, &resolution, &mut diagnostics);

  report(&mut diagnostics, &sources, options)?;

  // A library has no entry point to lower from until library outputs exist.
  if options.library {
    return Ok(FrontEnd { c_source: None });
  }

  let program = sagitario_sir::lower(&package, &resolution, &typed, &sources, &mut diagnostics).map_err(|error| {
    eprintln!("internal compiler error: {}", error.0);
    EXIT_ICE
  })?;

  // Generic code may instantiate itself endlessly, found while lowering.
  report(&mut diagnostics, &sources, options)?;

  if !generate {
    return Ok(FrontEnd { c_source: None });
  }
  Ok(FrontEnd { c_source: Some(sagitario_codegen_c::generate(&program)) })
}

/// The part of the prelude written in Sagitario.
const PRELUDE: &str = include_str!("prelude.sag");

/// The modules of the standard library written in Sagitario (docs/spec/11-standard-library.md),
/// loaded when a program names them.
/// The file of `std` itself: helpers shared by its modules, which programs cannot name.
const STD_ROOT: &str = include_str!("std/std.sag");

const STD_SOURCES: [(&str, &str); 6] = [
  ("env", include_str!("std/env.sag")),
  ("fs", include_str!("std/fs.sag")),
  ("io", include_str!("std/io.sag")),
  ("path", include_str!("std/path.sag")),
  ("process", include_str!("std/process.sag")),
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
    let base = options.output.as_ref().unwrap_or(&options.input);
    let c_path = base.with_extension("c");
    let ffi_path = c_path.with_file_name(format!("{}_ffi.c", base.file_stem().unwrap_or_default().to_string_lossy()));
    let written = fs::write(&c_path, &generated.program).and_then(|()| match &generated.ffi {
      Some(ffi) => fs::write(&ffi_path, ffi),
      None => Ok(()),
    });
    if let Err(error) = written {
      eprintln!("error: cannot write the generated C next to `{}`: {error}", options.input.display());
      return Err(EXIT_FAILURE);
    }
  }
  let mut compiler = find_c_compiler(options.cc.as_deref())?;
  compiler.sanitized = options.sanitize && compiler.supports_sanitizers();
  let dir = TempDir::create().map_err(|error| {
    eprintln!("error: cannot create a temporary directory: {error}");
    EXIT_FAILURE
  })?;
  let program_c = dir.path.join("program.c");
  let runtime_c = dir.path.join("sagitario_runtime.c");
  let ffi_c = dir.path.join("sag_ffi.c");
  let written = fs::write(&program_c, &generated.program)
    .and_then(|()| fs::write(dir.path.join(sagitario_codegen_c::RUNTIME_HEADER), RUNTIME_HEADER))
    .and_then(|()| fs::write(&runtime_c, RUNTIME_SOURCE))
    .and_then(|()| match &generated.ffi {
      Some(ffi) => fs::write(&ffi_c, ffi),
      None => Ok(()),
    });
  if let Err(error) = written {
    eprintln!("error: cannot write generated C to `{}`: {error}", dir.path.display());
    return Err(EXIT_FAILURE);
  }

  let mode: &[&str] = if options.release { &["-O2", "-DSAG_RELEASE"] } else { &["-O0", "-g"] };
  // `fmod` and friends live in libm outside Windows.
  let libm: &[&str] = if compiler.target.contains("windows") { &[] } else { &["-lm"] };
  let compile = |sanitize: &[&str]| {
    Command::new(&compiler.command)
      .args(["-std=c11", "-pedantic-errors"])
      .args(mode)
      .args(sanitize)
      .arg("-I")
      .arg(&dir.path)
      .arg(&program_c)
      .arg(&runtime_c)
      .args(generated.ffi.as_ref().map(|_| &ffi_c))
      .arg("-o")
      .arg(executable)
      .args(libm)
      .output()
  };
  let output = compile(if compiler.sanitized { &SANITIZE_FLAGS } else { &[] });
  match output {
    Ok(output) if output.status.success() => Ok(compiler),
    // The C compiles without sanitizers: the toolchain lacks their libraries.
    Ok(output) if compiler.sanitized && compile(&[]).is_ok_and(|plain| plain.status.success()) => {
      eprintln!("error: the C compiler `{}` cannot build with sanitizers (`{}`)", compiler.command, SANITIZE_FLAGS.join(" "));
      eprintln!("{}", String::from_utf8_lossy(&output.stderr));
      eprintln!("note: install its sanitizer libraries, or build without `--sanitize`");
      Err(EXIT_FAILURE)
    }
    Ok(output) => {
      eprintln!("internal compiler error: the generated C failed to compile with `{}`", compiler.command);
      eprintln!("{}", String::from_utf8_lossy(&output.stderr));
      Err(EXIT_ICE)
    }
    Err(error) => {
      eprintln!("error: cannot run the C compiler `{}`: {error}", compiler.command);
      Err(EXIT_FAILURE)
    }
  }
}

struct CCompiler {
  command: String,
  /// Target triple reported by `-dumpmachine`.
  target: String,
  /// Whether the program was compiled with sanitizers.
  sanitized: bool,
}

impl CCompiler {
  /// MinGW toolchains ship no sanitizer runtime (docs/adr/0018-windows-c-toolchain.md).
  fn supports_sanitizers(&self) -> bool {
    !self.target.contains("windows-gnu") && !self.target.contains("mingw")
  }

  /// The directory of the sanitizer runtime DLLs, which a sanitized program needs on `PATH` on
  /// Windows; `None` elsewhere or when the program is not sanitized.
  fn sanitizer_runtime_dir(&self) -> Option<PathBuf> {
    if !self.sanitized || !self.target.contains("windows") {
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
    return Ok(CCompiler { command, target, sanitized: false });
  }
  match configured {
    Some(command) => eprintln!("error: cannot run the C compiler `{command}` (from --cc or SAG_CC)"),
    None => eprintln!("error: no C compiler found; Sagitario needs `clang` (or `gcc`) on PATH\n{}", install_hint()),
  }
  Err(EXIT_FAILURE)
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
