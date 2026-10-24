//! `Nova.toml`, the manifest of a package (docs/nova.md#novatoml).

use sagitario_diagnostics::Diagnostic;
use sagitario_lexer::Keyword;
use sagitario_span::Span;

use crate::codes;
use crate::toml::{Document, Entry, Table, ValueKind};

/// The name of the manifest file.
pub const FILE_NAME: &str = "Nova.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
  pub name: String,
  pub version: String,
  pub description: Option<String>,
  pub authors: Vec<String>,
  pub license: Option<String>,
  /// The `[package]` header, which errors about the package as a whole point to.
  pub span: Span,
}

const PACKAGE_KEYS: [&str; 5] = ["name", "version", "description", "authors", "license"];

/// Path roots that cannot name a package, since packages become path roots too.
const RESERVED_NAMES: [&str; 5] = ["pkg", "self", "super", "std", "prelude"];

/// The manifest a parsed `Nova.toml` describes.
pub fn read(document: &Document) -> Result<Manifest, Box<Diagnostic>> {
  let mut package = None;

  for table in &document.tables {
    match table.name.as_str() {
      "" => {
        if let Some(entry) = table.entries.first() {
          return Err(Box::new(
            Diagnostic::error(codes::UNKNOWN_KEY, entry.span, format!("the key `{}` is outside any table", entry.key))
              .with_help("write the package's keys after a `[package]` header"),
          ));
        }
      }
      "package" => package = Some(table),
      "dependencies" => {
        return Err(Box::new(
          Diagnostic::error(codes::NOT_AVAILABLE, table.span, "dependencies are not available yet")
            .with_note("Nova builds packages without dependencies for now"),
        ));
      }
      name => {
        let mut diagnostic = Diagnostic::error(codes::UNKNOWN_KEY, table.span, format!("`Nova.toml` has no table `[{name}]`"));

        if let Some(similar) = sagitario_diagnostics::closest_match(name, ["package", "dependencies"]) {
          diagnostic = diagnostic.with_help(format!("a table with a similar name exists: `[{similar}]`"));
        }

        return Err(Box::new(diagnostic));
      }
    }
  }

  let Some(package) = package else {
    return Err(Box::new(
      Diagnostic::error(codes::MISSING_KEY, Span::new(document.file, 0, 0), "`Nova.toml` has no `[package]` table")
        .with_help("start it with `[package]`, then `name = \"...\"` and `version = \"0.1.0\"`"),
    ));
  };

  for entry in &package.entries {
    if !PACKAGE_KEYS.contains(&entry.key.as_str()) {
      let mut diagnostic = Diagnostic::error(codes::UNKNOWN_KEY, entry.span, format!("`[package]` has no key `{}`", entry.key));

      diagnostic = match sagitario_diagnostics::closest_match(&entry.key, PACKAGE_KEYS) {
        Some(similar) => diagnostic.with_help(format!("a key with a similar name exists: `{similar}`")),
        None => diagnostic.with_note(format!("its keys are {}", PACKAGE_KEYS.map(|key| format!("`{key}`")).join(", "))),
      };

      return Err(Box::new(diagnostic));
    }
  }

  let name = required_string(package, "name")?;
  let version = required_string(package, "version")?;

  check_name(&name.0, name.1)?;
  check_version(&version.0, version.1)?;

  let authors = match find(package, "authors") {
    None => Vec::new(),
    Some(entry) => match &entry.value.kind {
      ValueKind::Array(values) => {
        let mut authors = Vec::new();

        for value in values {
          match &value.kind {
            ValueKind::String(text) => authors.push(text.clone()),
            other => return Err(Box::new(wrong_type(value.span, "each author", "a string", other))),
          }
        }

        authors
      }
      other => return Err(Box::new(wrong_type(entry.value.span, "`authors`", "an array of strings", other))),
    },
  };

  Ok(Manifest {
    name: name.0,
    version: version.0,
    description: optional_string(package, "description")?,
    authors,
    license: optional_string(package, "license")?,
    span: package.span,
  })
}

fn find<'t>(table: &'t Table, key: &str) -> Option<&'t Entry> {
  table.entries.iter().find(|entry| entry.key == key)
}

fn wrong_type(span: Span, what: &str, expected: &str, found: &ValueKind) -> Diagnostic {
  Diagnostic::error(codes::WRONG_TYPE, span, format!("{what} must be {expected}, not {}", found.describe()))
}

/// A string key that must be present, with the span of its value.
fn required_string(table: &Table, key: &str) -> Result<(String, Span), Box<Diagnostic>> {
  let Some(entry) = find(table, key) else {
    let example = if key == "version" { "\"0.1.0\"" } else { "\"my_package\"" };

    return Err(Box::new(
      Diagnostic::error(codes::MISSING_KEY, table.span, format!("`[package]` has no `{key}`")).with_help(format!("add `{key} = {example}`")),
    ));
  };

  match &entry.value.kind {
    ValueKind::String(text) => Ok((text.clone(), entry.value.span)),
    other => Err(Box::new(wrong_type(entry.value.span, &format!("`{key}`"), "a string", other))),
  }
}

fn optional_string(table: &Table, key: &str) -> Result<Option<String>, Box<Diagnostic>> {
  match find(table, key) {
    None => Ok(None),
    Some(entry) => match &entry.value.kind {
      ValueKind::String(text) => Ok(Some(text.clone())),
      other => Err(Box::new(wrong_type(entry.value.span, &format!("`{key}`"), "a string", other))),
    },
  }
}

/// Why `name` cannot name a package, or `None` if it can: it must be a lowercase identifier in
/// snake_case that is neither a keyword nor a reserved path root.
pub fn name_problem(name: &str) -> Option<String> {
  let mut chars = name.chars();
  let valid = chars.next().is_some_and(|first| first.is_ascii_lowercase()) && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');

  if !valid {
    return Some(format!(
      "`{name}` is not a valid package name: it must start with a lowercase letter and have only lowercase letters, digits and `_`"
    ));
  }

  if Keyword::from_ident(name).is_some() || RESERVED_NAMES.contains(&name) {
    return Some(format!("`{name}` is reserved and cannot name a package"));
  }

  None
}

fn check_name(name: &str, span: Span) -> Result<(), Box<Diagnostic>> {
  match name_problem(name) {
    None => Ok(()),
    Some(problem) => {
      let suggestion: String = name.to_ascii_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
      let mut diagnostic =
        Diagnostic::error(codes::INVALID_NAME, span, problem).with_note("the name of a package is the path root other packages import it with");

      if name_problem(&suggestion).is_none() && suggestion != name {
        diagnostic = diagnostic.with_help(format!("use `{suggestion}`"));
      }

      Err(Box::new(diagnostic))
    }
  }
}

/// Whether `text` is a version as SemVer defines it: `MAJOR.MINOR.PATCH`, then optionally
/// `-pre.release` and `+build` identifiers.
pub fn is_valid_version(text: &str) -> bool {
  let (rest, build) = match text.split_once('+') {
    Some((rest, build)) => (rest, Some(build)),
    None => (text, None),
  };
  let (core, pre) = match rest.split_once('-') {
    Some((core, pre)) => (core, Some(pre)),
    None => (rest, None),
  };
  let numeric = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()) && (part == "0" || !part.starts_with('0'));
  let identifier = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
  let parts: Vec<&str> = core.split('.').collect();

  parts.len() == 3
    && parts.iter().all(|part| numeric(part))
    && pre.is_none_or(|pre| pre.split('.').all(|part| identifier(part) && (!part.bytes().all(|byte| byte.is_ascii_digit()) || numeric(part))))
    && build.is_none_or(|build| build.split('.').all(identifier))
}

fn check_version(version: &str, span: Span) -> Result<(), Box<Diagnostic>> {
  if is_valid_version(version) {
    return Ok(());
  }

  Err(Box::new(
    Diagnostic::error(codes::INVALID_VERSION, span, format!("`{version}` is not a valid version"))
      .with_note("versions follow Semantic Versioning: `MAJOR.MINOR.PATCH`, such as `1.4.0`, optionally followed by `-alpha.1`"),
  ))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::toml;
  use sagitario_span::SourceMap;
  use std::path::PathBuf;

  fn read_text(text: &str) -> Result<Manifest, Box<Diagnostic>> {
    let mut sources = SourceMap::new();
    let file = sources.add(PathBuf::from(FILE_NAME), text.to_string());

    read(&toml::parse(file, text)?)
  }

  #[test]
  fn reads_a_manifest() {
    let manifest =
      read_text("[package]\nname = \"demo_app\"\nversion = \"1.0.0-beta.2\"\nauthors = [\"Ana\", \"Luis\"]\nlicense = \"MIT\"\n").unwrap();

    assert_eq!(manifest.name, "demo_app");
    assert_eq!(manifest.version, "1.0.0-beta.2");
    assert_eq!(manifest.authors, vec!["Ana".to_string(), "Luis".to_string()]);
    assert_eq!(manifest.license.as_deref(), Some("MIT"));
    assert_eq!(manifest.description, None);
  }

  #[test]
  fn reports_each_kind_of_error() {
    let cases = [
      ("name = \"x\"", codes::UNKNOWN_KEY),
      ("[package]\nname = \"x\"\nversion = \"1.0.0\"\nedition = \"2024\"", codes::UNKNOWN_KEY),
      ("[packages]", codes::UNKNOWN_KEY),
      ("", codes::MISSING_KEY),
      ("[package]\nversion = \"1.0.0\"", codes::MISSING_KEY),
      ("[package]\nname = \"x\"", codes::MISSING_KEY),
      ("[package]\nname = 1\nversion = \"1.0.0\"", codes::WRONG_TYPE),
      ("[package]\nname = \"x\"\nversion = \"1.0.0\"\nauthors = \"Ana\"", codes::WRONG_TYPE),
      ("[package]\nname = \"x\"\nversion = \"1.0.0\"\nauthors = [1]", codes::WRONG_TYPE),
      ("[package]\nname = \"My-App\"\nversion = \"1.0.0\"", codes::INVALID_NAME),
      ("[package]\nname = \"class\"\nversion = \"1.0.0\"", codes::INVALID_NAME),
      ("[package]\nname = \"std\"\nversion = \"1.0.0\"", codes::INVALID_NAME),
      ("[package]\nname = \"x\"\nversion = \"1.0\"", codes::INVALID_VERSION),
      ("[package]\nname = \"x\"\nversion = \"1.0.0\"\n[dependencies]", codes::NOT_AVAILABLE),
    ];

    for (text, code) in cases {
      assert_eq!(read_text(text).unwrap_err().code, code, "{text}");
    }
  }

  #[test]
  fn checks_versions() {
    for valid in ["0.1.0", "1.20.300", "1.0.0-alpha", "1.0.0-alpha.1", "1.0.0-0.3.7", "1.0.0+build.5", "1.0.0-rc.1+abc"] {
      assert!(is_valid_version(valid), "{valid}");
    }

    for invalid in ["1", "1.0", "1.0.0.0", "01.0.0", "1.0.0-", "1.0.0-01", "1.0.0+", "v1.0.0", "1.a.0"] {
      assert!(!is_valid_version(invalid), "{invalid}");
    }
  }

  #[test]
  fn checks_names() {
    assert!(name_problem("my_app2").is_none());

    for invalid in ["", "My", "my-app", "2app", "_app", "fn", "pkg"] {
      assert!(name_problem(invalid).is_some(), "{invalid}");
    }
  }
}
