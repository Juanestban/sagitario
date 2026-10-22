//! Compilation driver: runs the pipeline (docs/architecture.md#pipeline), prints diagnostics,
//! writes the generated C with the embedded runtime, and invokes the C compiler.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

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
  c_source: Option<String>,
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
  let text = match fs::read_to_string(input) {
    Ok(text) => text,
    Err(error) => {
      eprintln!("error: cannot read `{}`: {error}", input.display());
      return Err(EXIT_FAILURE);
    }
  };
  if u32::try_from(text.len()).is_err() {
    eprintln!("error: `{}` is too large (the limit is 4 GiB)", input.display());
    return Err(EXIT_FAILURE);
  }

  let mut sources = SourceMap::new();
  let file = sources.add(input.clone(), text);
  let Some(source) = sources.get(file) else {
    eprintln!("internal compiler error: source file was not registered");
    return Err(EXIT_ICE);
  };
  let mut diagnostics = Diagnostics::new();

  let tokens = sagitario_lexer::lex(file, source.text(), &mut diagnostics);
  // Malformed tokens would only cause confusing syntax errors, so lexical errors stop here.
  report(&mut diagnostics, &sources, options)?;
  let module = sagitario_parser::parse_module(file, tokens, &mut diagnostics);
  report(&mut diagnostics, &sources, options)?;
  let resolution = sagitario_resolve::resolve(&module, &mut diagnostics);
  report(&mut diagnostics, &sources, options)?;
  let typed = sagitario_types::check(&module, &resolution, &mut diagnostics);
  report(&mut diagnostics, &sources, options)?;
  if !generate {
    return Ok(FrontEnd { c_source: None });
  }

  let program = sagitario_sir::lower(&module, &resolution, &typed, source).map_err(|error| {
    eprintln!("internal compiler error: {}", error.0);
    EXIT_ICE
  })?;
  Ok(FrontEnd { c_source: Some(sagitario_codegen_c::generate(&program)) })
}

/// Prints the collected diagnostics and stops the pipeline if any of them is an error.
fn report(diagnostics: &mut Diagnostics, sources: &SourceMap, options: &CompileOptions) -> Result<(), u8> {
  let errors = diagnostics.error_count();
  for diagnostic in diagnostics.take() {
    eprintln!("{}", render(&diagnostic, sources, options.diagnostic_format));
  }
  if errors == 0 {
    return Ok(());
  }
  if options.diagnostic_format == DiagnosticFormat::Human {
    let plural = if errors == 1 { "" } else { "s" };
    eprintln!("error: could not compile `{}` due to {errors} previous error{plural}", options.input.display());
  }
  Err(EXIT_FAILURE)
}

fn compile_to(options: &CompileOptions, executable: &Path) -> Result<(), u8> {
  let c_source = front_end(options, true)?.c_source.unwrap_or_default();
  if options.emit_c {
    let c_path = options.input.with_extension("c");
    if let Err(error) = fs::write(&c_path, &c_source) {
      eprintln!("error: cannot write `{}`: {error}", c_path.display());
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
  let written = fs::write(&program_c, &c_source)
    .and_then(|()| fs::write(dir.path.join(sagitario_codegen_c::RUNTIME_HEADER), RUNTIME_HEADER))
    .and_then(|()| fs::write(&runtime_c, RUNTIME_SOURCE));
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
