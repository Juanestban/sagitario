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

use sagitario_ast::{Module, Package};
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
}

/// `sag check`: runs the front end only.
pub fn check(options: &CompileOptions) -> u8 {
  match front_end(options, false) {
    Ok(_) => 0,
    Err(code) => code,
  }
}

/// `sag build`: writes an executable next to the source file.
pub fn build(options: &CompileOptions) -> u8 {
  let executable = options.input.with_extension(env::consts::EXE_EXTENSION);
  match compile_to(options, &executable) {
    Ok(()) => 0,
    Err(code) => code,
  }
}

/// `sag run`: builds into a temporary directory, runs the program and returns its exit code.
pub fn run(options: &CompileOptions) -> u8 {
  let dir = match TempDir::create() {
    Ok(dir) => dir,
    Err(error) => {
      eprintln!("error: cannot create a temporary directory: {error}");
      return EXIT_FAILURE;
    }
  };
  let stem = options.input.file_stem().unwrap_or_default();
  let executable = dir.path.join(stem).with_extension(env::consts::EXE_EXTENSION);
  if let Err(code) = compile_to(options, &executable) {
    return code;
  }
  match Command::new(&executable).args(&options.program_args).status() {
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
  let mut loader = Loader { root: input.parent().map(Path::to_path_buf).unwrap_or_default(), entry: input.clone(), next_id: 0, modules: Vec::new() };

  loader.load(input, Vec::new(), &mut sources, &mut diagnostics)?;

  // Each loaded module may refer to more modules; the ones that exist as files are loaded too.
  let mut attempted = HashSet::new();
  let mut index = 0;

  while let Some(module) = loader.modules.get(index) {
    for path in sagitario_resolve::module_references(module) {
      if attempted.insert(path.clone())
        && let Some(file) = loader.module_file(&path)
      {
        loader.load(&file, path, &mut sources, &mut diagnostics)?;
      }
    }

    index += 1;
  }

  // Malformed tokens or an incomplete syntax tree would only cause confusing follow-up errors,
  // so lexical and syntax errors stop here. Name, type and flow errors are reported together.
  report(&mut diagnostics, &sources, options)?;

  let prelude = loader.load_prelude(&mut sources, &mut diagnostics)?;

  report(&mut diagnostics, &sources, options)?;

  let package = Package { modules: loader.modules, prelude: Some(prelude) };
  let resolution = sagitario_resolve::resolve(&package, &mut diagnostics);
  let typed = sagitario_types::check(&package, &resolution, &mut diagnostics);

  report(&mut diagnostics, &sources, options)?;
  if !generate {
    return Ok(FrontEnd { c_source: None });
  }

  let program = sagitario_sir::lower(&package, &resolution, &typed, &sources).map_err(|error| {
    eprintln!("internal compiler error: {}", error.0);
    EXIT_ICE
  })?;
  Ok(FrontEnd { c_source: Some(sagitario_codegen_c::generate(&program)) })
}

/// The part of the prelude written in Sagitario.
const PRELUDE: &str = include_str!("prelude.sag");

/// Reads, lexes and parses the modules of a program (docs/spec/09-modules.md): the entry file is
/// the root module `pkg`, and `pkg::net::http` is the file `net/http.sag` next to it.
struct Loader {
  root: PathBuf,
  entry: PathBuf,
  /// Keeps node ids unique across modules.
  next_id: u32,
  modules: Vec<Module>,
}

impl Loader {
  /// Adds a module unless it has lexical errors, which are reported later with the others.
  fn load(&mut self, file: &Path, path: Vec<String>, sources: &mut SourceMap, diagnostics: &mut Diagnostics) -> Result<(), u8> {
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
      self.modules.push(module);
    }

    Ok(())
  }

  /// Adds the prelude module, which every program sees, and returns its index.
  fn load_prelude(&mut self, sources: &mut SourceMap, diagnostics: &mut Diagnostics) -> Result<usize, u8> {
    let id = sources.add(PathBuf::from("<prelude>"), PRELUDE.to_string());
    let Some(source) = sources.get(id) else {
      eprintln!("internal compiler error: the prelude was not registered");
      return Err(EXIT_ICE);
    };
    let tokens = sagitario_lexer::lex(id, source.text(), diagnostics);
    let mut module = sagitario_parser::parse_module_from(id, tokens, &mut self.next_id, diagnostics);

    module.path = vec!["std".to_string(), "prelude".to_string()];
    self.modules.push(module);

    Ok(self.modules.len() - 1)
  }

  /// The file of the module at `path`, if it exists with exactly these names (also on
  /// case-insensitive file systems) and is not the entry file.
  fn module_file(&self, path: &[String]) -> Option<PathBuf> {
    let (last, dirs) = path.split_last()?;
    let mut dir = self.root.clone();

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

    if same(&file, &self.entry) { None } else { Some(file) }
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
fn use_color(format: DiagnosticFormat) -> bool {
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

fn compile_to(options: &CompileOptions, executable: &Path) -> Result<(), u8> {
  let Some(generated) = front_end(options, true)?.c_source else {
    eprintln!("internal compiler error: no C was generated");
    return Err(EXIT_ICE);
  };
  if options.emit_c {
    let c_path = options.input.with_extension("c");
    let ffi_path = c_path.with_file_name(format!("{}_ffi.c", options.input.file_stem().unwrap_or_default().to_string_lossy()));
    let written = fs::write(&c_path, &generated.program).and_then(|()| match &generated.ffi {
      Some(ffi) => fs::write(&ffi_path, ffi),
      None => Ok(()),
    });
    if let Err(error) = written {
      eprintln!("error: cannot write the generated C next to `{}`: {error}", options.input.display());
      return Err(EXIT_FAILURE);
    }
  }
  let compiler = find_c_compiler(options.cc.as_deref())?;
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
  let output = Command::new(&compiler.command)
    .args(["-std=c11", "-pedantic-errors"])
    .args(mode)
    .arg("-I")
    .arg(&dir.path)
    .arg(&program_c)
    .arg(&runtime_c)
    .args(generated.ffi.as_ref().map(|_| &ffi_c))
    .arg("-o")
    .arg(executable)
    .args(libm)
    .output();
  match output {
    Ok(output) if output.status.success() => Ok(()),
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
    return Ok(CCompiler { command, target });
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
