//! Nova, the package manager of Sagitario (docs/nova.md): creates packages, reads `Nova.toml`
//! and builds, runs and checks packages with the compiler driver.

pub mod codes;
pub mod manifest;
pub mod toml;

use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sagitario_diagnostics::{Diagnostic, DiagnosticFormat, render};
use sagitario_driver::{CompileOptions, Dependency, EXIT_FAILURE, LibraryOutputs, Link};
use sagitario_span::SourceMap;

use manifest::Manifest;

pub const USAGE: &str = "\
Usage: nova <command> [options]

Commands:
  new <name>   Create a package in a new directory
  init         Create a package in the current directory
  build        Compile the package
  run          Compile and run the package (arguments after `--` go to the program)
  check        Check the package without generating code
  clean        Remove the `target` directory
  help         Show this message
  version      Show Nova's version

Options:
  --lib                       Create a library instead of a program (new, init)
  --name <name>               The package's name instead of the directory's (init)
  --release                   Optimize; integer overflow wraps instead of panicking (build, run)
  --sanitize                  Detect memory errors and undefined behavior with ASan and UBSan,
                              where the C compiler supports them (build, run)
  --emit-c                    Also write the generated C next to the executable (build, run)
  --cc <compiler>             C compiler to use; overrides SAG_CC (build, run)
  --diagnostic-format <fmt>   `human` (default) or `short`: one line per diagnostic";

/// Exit code for invalid command-line usage.
pub const EXIT_USAGE: u8 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
  New { name: String, lib: bool },
  Init { name: Option<String>, lib: bool },
  Build(Options),
  Run(Options),
  Check(Options),
  Clean,
  Help,
  Version,
}

/// The options of `build`, `run` and `check`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
  pub release: bool,
  pub sanitize: bool,
  pub emit_c: bool,
  pub cc: Option<String>,
  pub diagnostic_format: DiagnosticFormat,
  /// Arguments forwarded to the program by `nova run -- args`.
  pub program_args: Vec<String>,
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

/// Parses the arguments that follow the program name.
pub fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Result<Command, UsageError> {
  let mut args = args.into_iter();
  let Some(command) = args.next() else {
    return Err(usage_error("missing command"));
  };

  match command.as_str() {
    "new" | "init" => parse_creation(&command, args),
    "build" => parse_options(args, &command).map(Command::Build),
    "run" => parse_options(args, &command).map(Command::Run),
    "check" => parse_options(args, &command).map(Command::Check),
    "clean" => match args.next() {
      None => Ok(Command::Clean),
      Some(arg) => Err(usage_error(format!("unexpected argument `{arg}` for `nova clean`"))),
    },
    "help" | "--help" | "-h" => Ok(Command::Help),
    "version" | "--version" | "-v" => Ok(Command::Version),
    other => Err(usage_error(format!("unknown command `{other}`"))),
  }
}

/// `nova new <name> [--lib]` and `nova init [--name <name>] [--lib]`.
fn parse_creation<I: Iterator<Item = String>>(command: &str, mut args: I) -> Result<Command, UsageError> {
  let mut lib = false;
  let mut name = None;

  while let Some(arg) = args.next() {
    match arg.as_str() {
      "--lib" => lib = true,
      "--name" if command == "init" => name = Some(args.next().ok_or_else(|| usage_error("`--name` requires a value"))?),
      _ if arg.starts_with('-') => return Err(usage_error(format!("unknown option `{arg}` for `nova {command}`"))),
      _ if command == "new" && name.is_none() => name = Some(arg),
      _ => return Err(usage_error(format!("unexpected argument `{arg}` for `nova {command}`"))),
    }
  }

  if command == "init" {
    return Ok(Command::Init { name, lib });
  }

  let name = name.ok_or_else(|| usage_error("`nova new` needs the package's name: `nova new my_app`"))?;

  Ok(Command::New { name, lib })
}

fn parse_options<I: Iterator<Item = String>>(mut args: I, command: &str) -> Result<Options, UsageError> {
  let mut options = Options::default();
  let codegen = command != "check";

  while let Some(arg) = args.next() {
    let (flag, inline_value) = match arg.split_once('=') {
      Some((flag, value)) if flag.starts_with("--") => (flag.to_string(), Some(value.to_string())),
      _ => (arg.clone(), None),
    };
    let needs_codegen = matches!(flag.as_str(), "--release" | "--sanitize" | "--emit-c" | "--cc");

    if needs_codegen && !codegen {
      return Err(usage_error(format!("`{flag}` is not accepted by `nova check`, which generates no code")));
    }

    match flag.as_str() {
      "--release" => options.release = true,
      "--sanitize" => options.sanitize = true,
      "--emit-c" => options.emit_c = true,
      "--cc" => options.cc = Some(inline_value.or_else(|| args.next()).ok_or_else(|| usage_error("`--cc` requires a value"))?),
      "--diagnostic-format" => {
        options.diagnostic_format = match inline_value.or_else(|| args.next()).as_deref() {
          Some("human") => DiagnosticFormat::Human,
          Some("short") => DiagnosticFormat::Short,
          Some(other) => return Err(usage_error(format!("unknown diagnostic format `{other}`; expected `human` or `short`"))),
          None => return Err(usage_error("`--diagnostic-format` requires a value")),
        };
      }
      "--" if command == "run" => options.program_args = args.by_ref().collect(),
      "--" => return Err(usage_error("program arguments after `--` are only accepted by `nova run`")),
      _ if flag.starts_with('-') => return Err(usage_error(format!("unknown option `{flag}`"))),
      _ => return Err(usage_error(format!("unexpected argument `{arg}`; Nova builds the package of the current directory"))),
    }
  }

  Ok(options)
}

/// Runs a parsed command and returns the process exit code.
pub fn execute(command: &Command) -> ExitCode {
  let code = match command {
    Command::Help => {
      println!("{USAGE}");
      0
    }
    Command::Version => {
      println!("nova {}", env!("CARGO_PKG_VERSION"));
      0
    }
    Command::New { name, lib } => new_package(name, *lib),
    Command::Init { name, lib } => init_package(name.as_deref(), *lib),
    Command::Build(options) => compile(options, Mode::Build),
    Command::Run(options) => compile(options, Mode::Run),
    Command::Check(options) => compile(options, Mode::Check),
    Command::Clean => clean(),
  };

  ExitCode::from(code)
}

fn fail(message: impl fmt::Display) -> u8 {
  eprintln!("error: {message}");
  EXIT_FAILURE
}

const MAIN_SOURCE: &str = "fn main() {\n  println(\"Hello, world!\");\n}\n";

const LIB_SOURCE: &str = "/// Adds two numbers.\npub fn add(a: i32, b: i32) -> i32 {\n  a + b\n}\n";

fn manifest_source(name: &str) -> String {
  format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n")
}

/// The files of a new package, written in `dir` without replacing any that exists.
fn write_package(dir: &Path, name: &str, lib: bool) -> std::io::Result<()> {
  let src = dir.join("src");
  let write_new = |path: PathBuf, text: &str| if path.exists() { Ok(()) } else { fs::write(path, text) };

  fs::create_dir_all(&src)?;
  write_new(dir.join(manifest::FILE_NAME), &manifest_source(name))?;
  write_new(dir.join(".gitignore"), "/target\n")?;

  if !src.join("main.sag").exists() && !src.join("lib.sag").exists() {
    if lib {
      fs::write(src.join("lib.sag"), LIB_SOURCE)?;
    } else {
      fs::write(src.join("main.sag"), MAIN_SOURCE)?;
    }
  }

  Ok(())
}

fn kind_name(lib: bool) -> &'static str {
  if lib { "library" } else { "program" }
}

/// `nova new <name>`
fn new_package(name: &str, lib: bool) -> u8 {
  if let Some(problem) = manifest::name_problem(name) {
    return fail(problem);
  }

  let dir = PathBuf::from(name);

  if dir.exists() {
    return fail(format!("`{name}` already exists; choose another name, or run `nova init` inside it"));
  }

  if let Err(error) = write_package(&dir, name, lib) {
    return fail(format!("cannot create the package `{name}`: {error}"));
  }

  eprintln!("Created the {} package `{name}`", kind_name(lib));

  0
}

/// `nova init`
fn init_package(name: Option<&str>, lib: bool) -> u8 {
  let dir = match env::current_dir() {
    Ok(dir) => dir,
    Err(error) => return fail(format!("cannot read the current directory: {error}")),
  };

  if dir.join(manifest::FILE_NAME).exists() {
    return fail(format!("`{}` already exists in this directory", manifest::FILE_NAME));
  }

  let dir_name = dir.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default();
  let name = name.map(str::to_string).unwrap_or(dir_name);

  if let Some(problem) = manifest::name_problem(&name) {
    return fail(format!("{problem}\nhelp: choose a name with `nova init --name my_package`"));
  }

  // An existing `src/main.sag` or `src/lib.sag` decides the kind of package.
  let lib = if dir.join("src").join("main.sag").exists() { false } else { lib || dir.join("src").join("lib.sag").exists() };

  if let Err(error) = write_package(&dir, &name, lib) {
    return fail(format!("cannot create the package: {error}"));
  }

  eprintln!("Created the {} package `{name}`", kind_name(lib));

  0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
  Build,
  Run,
  Check,
}

/// A package of the graph being built: the one of the current directory, or a dependency.
struct Package {
  /// The directory of `Nova.toml`, as written in messages: relative when it is the current one.
  dir: PathBuf,
  /// The same directory, to recognize a package reached by several paths.
  canonical: PathBuf,
  manifest: Manifest,
  /// `src/main.sag` or `src/lib.sag`.
  root: PathBuf,
  lib: bool,
  /// Indices in the graph of its dependencies.
  dependencies: Vec<usize>,
}

/// The directory holding `Nova.toml`: the current one or the nearest above it.
fn find_package_dir() -> Result<PathBuf, u8> {
  let current = env::current_dir().map_err(|error| fail(format!("cannot read the current directory: {error}")))?;

  for dir in current.ancestors() {
    if dir.join(manifest::FILE_NAME).is_file() {
      // Messages show paths relative to the current directory when the package is there.
      return Ok(if dir == current { PathBuf::new() } else { dir.to_path_buf() });
    }
  }

  Err(fail(format!(
    "cannot find `{}` in `{}` or any directory above it\nhelp: create a package with `nova new my_app`, or `nova init` in an existing directory",
    manifest::FILE_NAME,
    current.display()
  )))
}

/// Reads the manifests of a package and its dependencies, reporting errors with their location.
struct Loader {
  sources: SourceMap,
  format: DiagnosticFormat,
  /// The package of the current directory first.
  packages: Vec<Package>,
}

impl Loader {
  fn report(&self, diagnostic: &Diagnostic) -> u8 {
    eprintln!("{}", render(diagnostic, &self.sources, self.format, sagitario_driver::use_color(self.format)));
    EXIT_FAILURE
  }

  /// Reads and checks the manifest and the layout of the package in `dir`.
  fn read(&mut self, dir: PathBuf) -> Result<Package, u8> {
    let path = dir.join(manifest::FILE_NAME);
    let text = fs::read_to_string(&path).map_err(|error| fail(format!("cannot read `{}`: {error}", path.display())))?;
    let here = if dir.as_os_str().is_empty() { Path::new(".") } else { dir.as_path() };
    let canonical = fs::canonicalize(here).map_err(|error| fail(format!("cannot read `{}`: {error}", here.display())))?;
    let file = self.sources.add(path, text.clone());
    let manifest = match toml::parse(file, &text).and_then(|document| manifest::read(&document)) {
      Ok(manifest) => manifest,
      Err(diagnostic) => return Err(self.report(&diagnostic)),
    };
    let src = dir.join("src");
    let main = src.join("main.sag");
    let lib = src.join("lib.sag");
    let layout_error = |message: String, help: &str| Diagnostic::error(codes::INVALID_LAYOUT, manifest.span, message).with_help(help.to_string());

    let (root, is_lib) = match (main.is_file(), lib.is_file()) {
      (true, false) => (main, false),
      (false, true) => (lib, true),
      (false, false) => {
        let diagnostic = layout_error(
          format!("the package `{}` has neither `src/main.sag` nor `src/lib.sag`", manifest.name),
          "create `src/main.sag` for a program, or `src/lib.sag` for a library",
        );

        return Err(self.report(&diagnostic));
      }
      (true, true) => {
        let diagnostic = layout_error(
          format!("the package `{}` has both `src/main.sag` and `src/lib.sag`", manifest.name),
          "a package is a program or a library for now: remove one of them",
        );

        return Err(self.report(&diagnostic));
      }
    };

    if let Some(library) = manifest.library.as_ref().filter(|_| !is_lib) {
      let diagnostic = Diagnostic::error(codes::NOT_APPLICABLE, library.span, format!("`{}` is a program, which has no `[library]`", manifest.name))
        .with_note("`[library]` chooses the files a library builds");

      return Err(self.report(&diagnostic));
    }

    for (file, span) in manifest.link.search_paths.iter().chain(&manifest.link.sources) {
      if !dir.join(file).exists() {
        let diagnostic = Diagnostic::error(codes::FILE_NOT_FOUND, *span, format!("`{}` does not exist", tidy(&dir.join(file)).display()))
          .with_note("paths in `[link]` are relative to the directory of `Nova.toml`");

        return Err(self.report(&diagnostic));
      }
    }

    Ok(Package { dir, canonical, manifest, root, lib: is_lib, dependencies: Vec::new() })
  }

  /// Loads the dependencies of the package at `index`, then theirs. `chain` holds the packages
  /// being loaded, from the first one: reaching one of them again is a cycle.
  fn load_dependencies(&mut self, index: usize, chain: &mut Vec<usize>) -> Result<(), u8> {
    let Some(package) = self.packages.get(index) else { return Ok(()) };
    let specs = package.manifest.dependencies.clone();
    let base = package.dir.clone();

    for spec in specs {
      let dir = tidy(&base.join(&spec.path));
      let canonical = fs::canonicalize(&dir).ok().filter(|canonical| canonical.join(manifest::FILE_NAME).is_file());
      let Some(canonical) = canonical else {
        let diagnostic = Diagnostic::error(codes::DEPENDENCY_NOT_FOUND, spec.path_span, format!("there is no package at `{}`", dir.display()))
          .with_note(format!("the dependency `{}` must be a directory with a `{}`", spec.name, manifest::FILE_NAME));

        return Err(self.report(&diagnostic));
      };

      let dependency = match self.packages.iter().position(|package| package.canonical == canonical) {
        Some(existing) if chain.contains(&existing) => {
          let start = chain.iter().position(|item| *item == existing).unwrap_or(0);
          let names: Vec<&str> = chain
            .iter()
            .skip(start)
            .chain([&existing])
            .filter_map(|item| self.packages.get(*item))
            .map(|package| package.manifest.name.as_str())
            .collect();
          let diagnostic =
            Diagnostic::error(codes::DEPENDENCY_CYCLE, spec.span, format!("the packages depend on each other: {}", names.join(" -> ")))
              .with_note("packages cannot depend on each other in a cycle; move what they share into another package");

          return Err(self.report(&diagnostic));
        }
        Some(existing) => existing,
        None => {
          let package = self.read(dir.clone())?;

          self.packages.push(package);

          let added = self.packages.len() - 1;

          chain.push(added);
          self.load_dependencies(added, chain)?;
          chain.pop();

          added
        }
      };
      let Some(found) = self.packages.get(dependency) else { return Err(EXIT_FAILURE) };

      if found.manifest.name != spec.name {
        let diagnostic = Diagnostic::error(
          codes::DEPENDENCY_NAME,
          spec.span,
          format!("the package at `{}` is called `{}`, not `{}`", dir.display(), found.manifest.name, spec.name),
        )
        .with_help(format!("write `{} = {{ path = \"{}\" }}`", found.manifest.name, spec.path));

        return Err(self.report(&diagnostic));
      }

      if !found.lib {
        let diagnostic = Diagnostic::error(codes::NOT_A_LIBRARY, spec.span, format!("`{}` is a program, not a library", spec.name))
          .with_note("only libraries (with `src/lib.sag`) can be dependencies");

        return Err(self.report(&diagnostic));
      }

      if let Some(package) = self.packages.get_mut(index) {
        package.dependencies.push(dependency);
      }
    }

    Ok(())
  }
}

/// `path` without `.` and with each `..` removing the directory before it, for messages:
/// `../geo/../math` becomes `../math`. It never looks at the file system.
fn tidy(path: &Path) -> PathBuf {
  use std::path::Component;

  let mut tidied = PathBuf::new();

  for component in path.components() {
    match component {
      Component::CurDir => {}
      Component::ParentDir if matches!(tidied.components().next_back(), Some(Component::Normal(_))) => {
        tidied.pop();
      }
      other => tidied.push(other),
    }
  }

  if tidied.as_os_str().is_empty() { PathBuf::from(".") } else { tidied }
}

/// Reads the package of the current directory and its dependencies.
fn load_packages(format: DiagnosticFormat) -> Result<Vec<Package>, u8> {
  let dir = find_package_dir()?;
  let mut loader = Loader { sources: SourceMap::new(), format, packages: Vec::new() };
  let package = loader.read(dir)?;

  loader.packages.push(package);
  loader.load_dependencies(0, &mut vec![0])?;

  Ok(loader.packages)
}

/// `nova build`, `nova run` and `nova check`.
fn compile(options: &Options, mode: Mode) -> u8 {
  let packages = match load_packages(options.diagnostic_format) {
    Ok(packages) => packages,
    Err(code) => return code,
  };
  let Some(package) = packages.first() else { return EXIT_FAILURE };
  // The compiler numbers dependencies from 0: package `i` of the graph is dependency `i - 1`.
  let dependencies = packages
    .iter()
    .skip(1)
    .map(|dependency| Dependency {
      name: dependency.manifest.name.clone(),
      entry: dependency.root.clone(),
      dependencies: dependency.dependencies.iter().map(|index| index - 1).collect(),
    })
    .collect();
  let profile = if options.release { "release" } else { "debug" };
  let target = package.dir.join("target").join(profile);
  let executable = target.join(&package.manifest.name).with_extension(env::consts::EXE_EXTENSION);
  // `[link]` of every package of the graph applies to the whole program.
  let mut link = Link::default();

  for member in &packages {
    for library in &member.manifest.link.libraries {
      if !link.libraries.contains(library) {
        link.libraries.push(library.clone());
      }
    }

    link.search_paths.extend(member.manifest.link.search_paths.iter().map(|(path, _)| tidy(&member.dir.join(path))));
    link.sources.extend(member.manifest.link.sources.iter().map(|(path, _)| tidy(&member.dir.join(path))));
  }
  let compile_options = CompileOptions {
    input: package.root.clone(),
    emit_c: options.emit_c,
    cc: options.cc.clone(),
    diagnostic_format: options.diagnostic_format,
    program_args: options.program_args.clone(),
    release: options.release,
    sanitize: options.sanitize,
    output: Some(executable.clone()),
    library: package.lib,
    dependencies,
    direct_dependencies: package.dependencies.iter().map(|index| index - 1).collect(),
    name: package.manifest.name.clone(),
    link,
  };

  for dependency in packages.iter().skip(1) {
    eprintln!("Using {} v{} ({})", dependency.manifest.name, dependency.manifest.version, dependency.dir.display());
  }
  let Manifest { name, version, .. } = &package.manifest;

  if package.lib && mode == Mode::Run {
    return fail(format!("`{name}` is a library, which cannot run; `nova run` runs packages with `src/main.sag`"));
  }

  if mode == Mode::Check {
    eprintln!("Checking {name} v{version}");

    return sagitario_driver::check(&compile_options);
  }

  if let Err(error) = fs::create_dir_all(&target) {
    return fail(format!("cannot create `{}`: {error}", target.display()));
  }

  eprintln!("Compiling {name} v{version}");

  if package.lib {
    let library = package.manifest.library.as_ref();
    let outputs = LibraryOutputs {
      dir: target.clone(),
      static_lib: library.is_none_or(|library| library.static_lib),
      dynamic_lib: library.is_none_or(|library| library.dynamic_lib),
    };

    return match sagitario_driver::build_library(&compile_options, &outputs) {
      Ok(files) => {
        let files: Vec<String> = files.iter().map(|file| file.display().to_string()).collect();

        eprintln!("Finished {profile} library: {}", files.join(", "));
        0
      }
      Err(code) => code,
    };
  }

  if mode == Mode::Run {
    return sagitario_driver::run(&compile_options);
  }

  let code = sagitario_driver::build(&compile_options);

  if code == 0 {
    eprintln!("Finished {profile} build: {}", executable.display());
  }

  code
}

/// `nova clean`
fn clean() -> u8 {
  let dir = match find_package_dir() {
    Ok(dir) => dir,
    Err(code) => return code,
  };
  let target = dir.join("target");

  if !target.exists() {
    return 0;
  }

  match fs::remove_dir_all(&target) {
    Ok(()) => {
      eprintln!("Removed {}", target.display());
      0
    }
    Err(error) => fail(format!("cannot remove `{}`: {error}", target.display())),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn parse(args: &[&str]) -> Result<Command, UsageError> {
    parse_args(args.iter().map(|arg| arg.to_string()))
  }

  #[test]
  fn parses_creation_commands() {
    assert_eq!(parse(&["new", "app"]), Ok(Command::New { name: "app".to_string(), lib: false }));
    assert_eq!(parse(&["new", "--lib", "util"]), Ok(Command::New { name: "util".to_string(), lib: true }));
    assert_eq!(parse(&["init"]), Ok(Command::Init { name: None, lib: false }));
    assert_eq!(parse(&["init", "--name", "app", "--lib"]), Ok(Command::Init { name: Some("app".to_string()), lib: true }));
    assert!(parse(&["new"]).is_err());
    assert!(parse(&["new", "a", "b"]).is_err());
    assert!(parse(&["new", "--name", "a"]).is_err());
    assert!(parse(&["init", "app"]).is_err());
  }

  #[test]
  fn parses_build_options() {
    let expected = Options { release: true, sanitize: true, cc: Some("gcc".to_string()), ..Options::default() };

    assert_eq!(parse(&["build", "--release", "--sanitize", "--cc=gcc"]), Ok(Command::Build(expected)));

    let run = Options { program_args: vec!["a".to_string(), "--b".to_string()], ..Options::default() };

    assert_eq!(parse(&["run", "--", "a", "--b"]), Ok(Command::Run(run)));
    assert_eq!(
      parse(&["check", "--diagnostic-format", "short"]),
      Ok(Command::Check(Options { diagnostic_format: DiagnosticFormat::Short, ..Options::default() }))
    );
  }

  #[test]
  fn rejects_invalid_usage() {
    assert!(parse(&[]).is_err());
    assert!(parse(&["publish"]).is_err());
    assert!(parse(&["build", "main.sag"]).is_err());
    assert!(parse(&["build", "--", "x"]).is_err());
    assert!(parse(&["check", "--release"]).is_err());
    assert!(parse(&["build", "--cc"]).is_err());
    assert!(parse(&["clean", "x"]).is_err());
  }
}
