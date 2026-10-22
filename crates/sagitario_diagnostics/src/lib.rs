//! Compiler diagnostics: codes, collection and rendering.

pub mod codes;

use sagitario_span::{SourceMap, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
  Error,
  Warning,
}

impl Severity {
  fn label(self) -> &'static str {
    match self {
      Severity::Error => "error",
      Severity::Warning => "warning",
    }
  }
}

/// A registered diagnostic code, such as `E0001`. See [`codes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Code(pub &'static str);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
  pub severity: Severity,
  pub code: Code,
  pub message: String,
  pub span: Span,
  pub notes: Vec<String>,
}

impl Diagnostic {
  pub fn error(code: Code, span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic { severity: Severity::Error, code, message: message.into(), span, notes: Vec::new() }
  }

  pub fn warning(code: Code, span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic { severity: Severity::Warning, code, message: message.into(), span, notes: Vec::new() }
  }

  pub fn with_note(mut self, note: impl Into<String>) -> Diagnostic {
    self.notes.push(note.into());

    self
  }
}

#[derive(Debug, Default)]
pub struct Diagnostics {
  items: Vec<Diagnostic>,
}

impl Diagnostics {
  pub fn new() -> Diagnostics {
    Diagnostics::default()
  }

  pub fn push(&mut self, diagnostic: Diagnostic) {
    self.items.push(diagnostic);
  }

  pub fn has_errors(&self) -> bool {
    self.items.iter().any(|diagnostic| diagnostic.severity == Severity::Error)
  }

  pub fn error_count(&self) -> usize {
    self.items.iter().filter(|diagnostic| diagnostic.severity == Severity::Error).count()
  }

  pub fn is_empty(&self) -> bool {
    self.items.is_empty()
  }

  pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
    self.items.iter()
  }

  /// Removes and returns every collected diagnostic.
  pub fn take(&mut self) -> Vec<Diagnostic> {
    std::mem::take(&mut self.items)
  }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DiagnosticFormat {
  /// Multi-line diagnostics with source excerpts, for people.
  #[default]
  Human,
  /// One line per diagnostic, `path:line:column: error[E0001]: message`, for tools and tests.
  Short,
}

pub fn render(diagnostic: &Diagnostic, sources: &SourceMap, format: DiagnosticFormat) -> String {
  match format {
    DiagnosticFormat::Human => render_human(diagnostic, sources),
    DiagnosticFormat::Short => render_short(diagnostic, sources),
  }
}

/// `path:line:column: error[E0001]: message`, without a trailing newline.
pub fn render_short(diagnostic: &Diagnostic, sources: &SourceMap) -> String {
  let location = match sources.get(diagnostic.span.file) {
    Some(file) => {
      let position = file.line_col(diagnostic.span.start);

      format!("{}:{}:{}", file.path().display(), position.line, position.column)
    }
    None => "<unknown>:0:0".to_string(),
  };

  format!("{location}: {}[{}]: {}", diagnostic.severity.label(), diagnostic.code.0, diagnostic.message)
}

/// A rustc-style multi-line rendering with a source excerpt, ending with a newline.
pub fn render_human(diagnostic: &Diagnostic, sources: &SourceMap) -> String {
  let mut out = format!("{}[{}]: {}\n", diagnostic.severity.label(), diagnostic.code.0, diagnostic.message);
  let Some(file) = sources.get(diagnostic.span.file) else {
    return out;
  };
  let position = file.line_col(diagnostic.span.start);
  let line_number = position.line.to_string();
  let pad = " ".repeat(line_number.len());
  let line_text = file.line_text(position.line);
  // Keep tabs so the carets line up with the source line.
  let prefix: String = line_text.chars().take(position.column.saturating_sub(1) as usize).map(|c| if c == '\t' { '\t' } else { ' ' }).collect();
  let underlined = file.slice(diagnostic.span).split('\n').next().unwrap_or("");
  let carets = "^".repeat(underlined.chars().count().max(1));

  out.push_str(&format!("{pad}--> {}:{}:{}\n", file.path().display(), position.line, position.column));
  out.push_str(&format!("{pad} |\n"));
  out.push_str(&format!("{line_number} | {line_text}\n"));
  out.push_str(&format!("{pad} | {prefix}{carets}\n"));

  for note in &diagnostic.notes {
    out.push_str(&format!("{pad} = note: {note}\n"));
  }

  out
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::path::PathBuf;

  fn sample() -> (SourceMap, Diagnostic) {
    let mut sources = SourceMap::new();
    let file = sources.add(PathBuf::from("main.sag"), "fn main() {\n  println(42);\n}\n".to_string());
    let diagnostic = Diagnostic::error(codes::MISMATCHED_TYPES, Span::new(file, 22, 24), "expected `String`, found `i32`")
      .with_note("string literals are written between double quotes");

    (sources, diagnostic)
  }

  #[test]
  fn renders_short_format() {
    let (sources, diagnostic) = sample();

    assert_eq!(render_short(&diagnostic, &sources), "main.sag:2:11: error[E0300]: expected `String`, found `i32`");
  }

  #[test]
  fn renders_human_format() {
    let (sources, diagnostic) = sample();
    let expected = "\
error[E0300]: expected `String`, found `i32`
 --> main.sag:2:11
  |
2 |   println(42);
  |           ^^
  = note: string literals are written between double quotes
";
    assert_eq!(render_human(&diagnostic, &sources), expected);
  }

  #[test]
  fn counts_errors_only() {
    let (_, diagnostic) = sample();
    let mut diagnostics = Diagnostics::new();

    diagnostics.push(Diagnostic::warning(codes::NOT_IMPLEMENTED, diagnostic.span, "w"));
    assert!(!diagnostics.has_errors());

    diagnostics.push(diagnostic);
    assert_eq!(diagnostics.error_count(), 1);
  }
}
