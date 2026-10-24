//! Compiler diagnostics: codes, collection and rendering.

pub mod codes;

use sagitario_span::{SourceFile, SourceMap, Span};

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

/// A secondary location shown with the main one, such as "previous definition here".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
  pub span: Span,
  pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
  pub severity: Severity,
  pub code: Code,
  pub message: String,
  pub span: Span,
  pub labels: Vec<Label>,
  pub notes: Vec<String>,
  /// Suggestions on how to fix the problem.
  pub help: Vec<String>,
}

impl Diagnostic {
  pub fn error(code: Code, span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Severity::Error, code, span, message.into())
  }

  pub fn warning(code: Code, span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Severity::Warning, code, span, message.into())
  }

  fn new(severity: Severity, code: Code, span: Span, message: String) -> Diagnostic {
    Diagnostic { severity, code, message, span, labels: Vec::new(), notes: Vec::new(), help: Vec::new() }
  }

  pub fn with_label(mut self, span: Span, message: impl Into<String>) -> Diagnostic {
    self.labels.push(Label { span, message: message.into() });

    self
  }

  pub fn with_note(mut self, note: impl Into<String>) -> Diagnostic {
    self.notes.push(note.into());

    self
  }

  pub fn with_help(mut self, help: impl Into<String>) -> Diagnostic {
    self.help.push(help.into());

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

  /// The number of diagnostics, errors and warnings.
  pub fn len(&self) -> usize {
    self.items.len()
  }

  /// Drops the diagnostics after the first `len`: those of an attempt the parser abandons.
  pub fn truncate(&mut self, len: usize) {
    self.items.truncate(len);
  }

  pub fn has_errors(&self) -> bool {
    self.items.iter().any(|diagnostic| diagnostic.severity == Severity::Error)
  }

  pub fn error_count(&self) -> usize {
    self.count(Severity::Error)
  }

  pub fn warning_count(&self) -> usize {
    self.count(Severity::Warning)
  }

  fn count(&self, severity: Severity) -> usize {
    self.items.iter().filter(|diagnostic| diagnostic.severity == severity).count()
  }

  pub fn is_empty(&self) -> bool {
    self.items.is_empty()
  }

  pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
    self.items.iter()
  }

  /// Removes and returns every collected diagnostic, ordered by position in the source.
  pub fn take(&mut self) -> Vec<Diagnostic> {
    let mut items = std::mem::take(&mut self.items);

    // Stable, so diagnostics at the same position keep the order of the phases.
    items.sort_by_key(|diagnostic| (diagnostic.span.file, diagnostic.span.start));
    items
  }
}

/// The closest of `candidates` to `name` by edit distance, if it is close enough to be a likely
/// typo (at most a third of the name's length, and at least one edit).
pub fn closest_match<'a>(name: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
  let limit = (name.chars().count() / 3).max(1);

  candidates
    .into_iter()
    .filter(|candidate| *candidate != name)
    .map(|candidate| (edit_distance(name, candidate), candidate))
    .filter(|(distance, _)| *distance <= limit)
    .min_by_key(|(distance, _)| *distance)
    .map(|(_, candidate)| candidate)
}

/// Levenshtein distance over Unicode scalar values.
fn edit_distance(a: &str, b: &str) -> usize {
  let b: Vec<char> = b.chars().collect();
  let mut previous: Vec<usize> = (0..=b.len()).collect();

  for (i, ca) in a.chars().enumerate() {
    let mut current = vec![i + 1];

    for (j, cb) in b.iter().enumerate() {
      let substitution = previous.get(j).copied().unwrap_or(0) + usize::from(ca != *cb);
      let deletion = previous.get(j + 1).copied().unwrap_or(0) + 1;
      let insertion = current.get(j).copied().unwrap_or(0) + 1;

      current.push(substitution.min(deletion).min(insertion));
    }

    previous = current;
  }

  previous.last().copied().unwrap_or(0)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DiagnosticFormat {
  /// Multi-line diagnostics with source excerpts, for people.
  #[default]
  Human,
  /// One line per diagnostic, `path:line:column: error[E0001]: message`, for tools and tests.
  Short,
}

/// Renders a diagnostic; `color` adds ANSI colors to the human format.
pub fn render(diagnostic: &Diagnostic, sources: &SourceMap, format: DiagnosticFormat, color: bool) -> String {
  match format {
    DiagnosticFormat::Human => render_human(diagnostic, sources, color),
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

/// ANSI styles; all empty when color is off.
struct Style {
  severity: &'static str,
  primary: &'static str,
  secondary: &'static str,
  help: &'static str,
  bold: &'static str,
  reset: &'static str,
}

impl Style {
  fn new(severity: Severity, color: bool) -> Style {
    if !color {
      return Style { severity: "", primary: "", secondary: "", help: "", bold: "", reset: "" };
    }

    let (severity, primary) = match severity {
      Severity::Error => ("\x1b[1;31m", "\x1b[1;31m"),
      Severity::Warning => ("\x1b[1;33m", "\x1b[1;33m"),
    };

    Style { severity, primary, secondary: "\x1b[1;34m", help: "\x1b[1;32m", bold: "\x1b[1m", reset: "\x1b[0m" }
  }
}

/// One underline on a source line: `^^^` for the main span, `---` for labels.
struct Marker<'a> {
  line: u32,
  column: u32,
  width: usize,
  primary: bool,
  message: Option<&'a str>,
}

fn marker<'a>(file: &SourceFile, span: Span, primary: bool, message: Option<&'a str>) -> Marker<'a> {
  let position = file.line_col(span.start);
  let underlined = file.slice(span).split('\n').next().unwrap_or("");

  Marker { line: position.line, column: position.column, width: underlined.chars().count().max(1), primary, message }
}

/// A rustc-style multi-line rendering with source excerpts, ending with a newline.
pub fn render_human(diagnostic: &Diagnostic, sources: &SourceMap, color: bool) -> String {
  let style = Style::new(diagnostic.severity, color);
  let mut out = format!(
    "{}{}[{}]{}{}: {}{}\n",
    style.severity,
    diagnostic.severity.label(),
    diagnostic.code.0,
    style.reset,
    style.bold,
    diagnostic.message,
    style.reset
  );
  let Some(file) = sources.get(diagnostic.span.file) else {
    return out;
  };
  // Source excerpts, one per file: the primary span's file first, then the files of labels
  // pointing elsewhere (such as a declaration in another module).
  let mut groups = vec![(diagnostic.span.file, file, vec![marker(file, diagnostic.span, true, None)])];
  let mut unplaced = Vec::new();

  for label in &diagnostic.labels {
    let Some(label_file) = sources.get(label.span.file) else {
      unplaced.push(&label.message);
      continue;
    };
    let label_marker = marker(label_file, label.span, false, Some(&label.message));

    match groups.iter_mut().find(|(id, _, _)| *id == label.span.file) {
      Some((_, _, markers)) => markers.push(label_marker),
      None => groups.push((label.span.file, label_file, vec![label_marker])),
    }
  }

  let main = file.line_col(diagnostic.span.start);
  let width = groups.iter().flat_map(|(_, _, markers)| markers.iter().map(|marker| marker.line)).max().unwrap_or(main.line).to_string().len();
  let pad = " ".repeat(width);
  let gutter = |text: &str| format!("{}{text}{}", style.secondary, style.reset);

  for (index, (_, file, markers)) in groups.iter_mut().enumerate() {
    markers.sort_by_key(|marker| (marker.line, marker.column));

    let (arrow, line, column) = match markers.first() {
      Some(first) if index > 0 => (":::", first.line, first.column),
      _ => ("-->", main.line, main.column),
    };

    out.push_str(&format!("{pad}{} {}:{line}:{column}\n", gutter(arrow), file.path().display()));
    out.push_str(&format!("{pad} {}\n", gutter("|")));

    let mut previous_line: Option<u32> = None;

    for current in markers.iter() {
      if previous_line != Some(current.line) {
        if previous_line.is_some_and(|previous| current.line > previous + 1) {
          out.push_str(&format!("{}\n", gutter("...")));
        }

        out.push_str(&format!("{} {}\n", gutter(&format!("{:>width$} |", current.line)), file.line_text(current.line)));
        previous_line = Some(current.line);
      }

      let line_text = file.line_text(current.line);
      // Keep tabs so the underline lines up with the source line.
      let prefix: String = line_text.chars().take(current.column.saturating_sub(1) as usize).map(|c| if c == '\t' { '\t' } else { ' ' }).collect();
      let (symbol, color) = if current.primary { ("^", style.primary) } else { ("-", style.secondary) };
      let message = current.message.map_or(String::new(), |message| format!(" {message}"));

      out.push_str(&format!("{pad} {} {prefix}{color}{}{message}{}\n", gutter("|"), symbol.repeat(current.width), style.reset));
    }
  }

  for note in diagnostic.notes.iter().chain(unplaced) {
    out.push_str(&format!("{pad} {} {}note{}: {note}\n", gutter("="), style.bold, style.reset));
  }

  for help in &diagnostic.help {
    out.push_str(&format!("{pad} {} {}help{}: {help}\n", gutter("="), style.help, style.reset));
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

    assert_eq!(render_human(&diagnostic, &sources, false), expected);
  }

  #[test]
  fn renders_labels_in_other_files() {
    let mut sources = SourceMap::new();
    let main = sources.add(PathBuf::from("main.sag"), "use pkg::vault::hidden;\n".to_string());
    let vault = sources.add(PathBuf::from("vault.sag"), "fn hidden() {}\n".to_string());
    let diagnostic = Diagnostic::error(codes::PRIVATE_ITEM, Span::new(main, 16, 22), "`hidden` is private")
      .with_label(Span::new(vault, 3, 9), "declared without `pub` here");
    let expected = "\
error[E0207]: `hidden` is private
 --> main.sag:1:17
  |
1 | use pkg::vault::hidden;
  |                 ^^^^^^
 ::: vault.sag:1:4
  |
1 | fn hidden() {}
  |    ------ declared without `pub` here
";

    assert_eq!(render_human(&diagnostic, &sources, false), expected);
  }

  #[test]
  fn renders_labels_on_other_lines_and_help() {
    let mut sources = SourceMap::new();
    let text = "fn f() {}\n\n\n\nfn f() {}\n";
    let file = sources.add(PathBuf::from("main.sag"), text.to_string());
    let diagnostic = Diagnostic::error(codes::DUPLICATE_DEFINITION, Span::new(file, 16, 17), "`f` is defined more than once")
      .with_label(Span::new(file, 3, 4), "previous definition here")
      .with_help("rename one of the functions");
    let expected = "\
error[E0201]: `f` is defined more than once
 --> main.sag:5:4
  |
1 | fn f() {}
  |    - previous definition here
...
5 | fn f() {}
  |    ^
  = help: rename one of the functions
";

    assert_eq!(render_human(&diagnostic, &sources, false), expected);
  }

  #[test]
  fn colors_only_when_asked() {
    let (sources, diagnostic) = sample();

    assert!(render_human(&diagnostic, &sources, true).starts_with("\x1b[1;31merror[E0300]"));
    assert!(!render_human(&diagnostic, &sources, false).contains('\x1b'));
  }

  #[test]
  fn sorts_by_position_and_counts() {
    let (_, diagnostic) = sample();
    let later = diagnostic.clone();
    let earlier = Diagnostic::warning(codes::UNUSED_BINDING, Span::new(diagnostic.span.file, 1, 2), "w");
    let mut diagnostics = Diagnostics::new();

    diagnostics.push(later);
    diagnostics.push(earlier);

    assert!(diagnostics.has_errors());
    assert_eq!((diagnostics.error_count(), diagnostics.warning_count()), (1, 1));

    let codes: Vec<&str> = diagnostics.take().iter().map(|diagnostic| diagnostic.code.0).collect();

    assert_eq!(codes, vec!["W0001", "E0300"]);
  }

  #[test]
  fn suggests_close_names() {
    assert_eq!(closest_match("prnt", ["print", "println", "eprintln"]), Some("print"));
    assert_eq!(closest_match("lenght", ["length", "width"]), Some("length"));
    assert_eq!(closest_match("x", ["y", "foo"]), Some("y"));
    assert_eq!(closest_match("total", ["println", "main"]), None);
    assert_eq!(closest_match("same", ["same"]), None);
  }
}
