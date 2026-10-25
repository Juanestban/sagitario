//! The subset of TOML that `Nova.toml` uses (docs/nova.md#novatoml): tables, `key = value` with
//! strings, integers, booleans and arrays, and comments. Other TOML features are reported as
//! unsupported, never guessed.

use sagitario_diagnostics::Diagnostic;
use sagitario_span::{FileId, Span};

use crate::codes;

/// A parsed file: the keys before any table header form the table named `""`, which comes first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
  pub file: FileId,
  pub tables: Vec<Table>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
  pub name: String,
  /// The header `[name]`; empty for the keys before any header.
  pub span: Span,
  pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
  pub key: String,
  pub span: Span,
  pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value {
  pub kind: ValueKind,
  pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueKind {
  String(String),
  Integer(i64),
  Boolean(bool),
  Array(Vec<Value>),
  /// `{ key = value, ... }`, on one line.
  Table(Vec<Entry>),
}

impl ValueKind {
  /// The kind of value, for messages: "a string".
  pub fn describe(&self) -> &'static str {
    match self {
      ValueKind::String(_) => "a string",
      ValueKind::Integer(_) => "an integer",
      ValueKind::Boolean(_) => "a boolean",
      ValueKind::Array(_) => "an array",
      ValueKind::Table(_) => "an inline table",
    }
  }
}

/// Parses `text`, stopping at the first error.
pub fn parse(file: FileId, text: &str) -> Result<Document, Box<Diagnostic>> {
  let mut parser = Parser { file, text, pos: 0 };
  let mut tables = vec![Table { name: String::new(), span: Span::new(file, 0, 0), entries: Vec::new() }];

  loop {
    parser.skip_blank(true);

    let Some(c) = parser.peek() else { break };

    if c == '[' {
      let table = parser.header()?;

      if let Some(previous) = tables.iter().find(|previous| previous.name == table.name) {
        return Err(Box::new(
          Diagnostic::error(codes::DUPLICATE, table.span, format!("the table `[{}]` is defined twice", table.name))
            .with_label(previous.span, "first defined here"),
        ));
      }

      tables.push(table);
    } else {
      let entry = parser.entry()?;
      let Some(table) = tables.last_mut() else { break };

      if let Some(previous) = table.entries.iter().find(|previous| previous.key == entry.key) {
        return Err(Box::new(
          Diagnostic::error(codes::DUPLICATE, entry.span, format!("the key `{}` is defined twice", entry.key))
            .with_label(previous.span, "first defined here"),
        ));
      }

      table.entries.push(entry);
    }
  }

  Ok(Document { file, tables })
}

struct Parser<'a> {
  file: FileId,
  text: &'a str,
  /// A byte offset at a character boundary.
  pos: usize,
}

impl Parser<'_> {
  fn peek(&self) -> Option<char> {
    self.text.get(self.pos..).and_then(|rest| rest.chars().next())
  }

  fn peek_at(&self, offset: usize) -> Option<char> {
    self.text.get(self.pos..).and_then(|rest| rest.chars().nth(offset))
  }

  fn bump(&mut self) -> Option<char> {
    let c = self.peek()?;

    self.pos += c.len_utf8();

    Some(c)
  }

  fn span(&self, start: usize) -> Span {
    Span::new(self.file, start as u32, self.pos as u32)
  }

  /// A span of one character (or the end of the file) at the current position.
  fn here(&self) -> Span {
    let len = self.peek().map_or(0, char::len_utf8);

    Span::new(self.file, self.pos as u32, (self.pos + len) as u32)
  }

  fn error(&self, span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::error(codes::SYNTAX, span, message)
  }

  fn unsupported(&self, span: Span, what: &str) -> Diagnostic {
    self
      .error(span, format!("{what} are not supported in `Nova.toml`"))
      .with_note("`Nova.toml` uses a subset of TOML: tables, `key = value` with strings, integers, booleans and arrays, and comments")
  }

  /// Skips spaces, tabs and comments, and line breaks too with `newlines`.
  fn skip_blank(&mut self, newlines: bool) {
    while let Some(c) = self.peek() {
      match c {
        ' ' | '\t' => {
          self.bump();
        }
        '\r' | '\n' if newlines => {
          self.bump();
        }
        '#' => {
          while self.peek().is_some_and(|c| c != '\n') {
            self.bump();
          }
        }
        _ => break,
      }
    }
  }

  /// The end of a line after a header or an entry, with an optional comment.
  fn line_end(&mut self) -> Result<(), Box<Diagnostic>> {
    self.skip_blank(false);

    match self.peek() {
      None | Some('\n') => Ok(()),
      Some('\r') if self.peek_at(1) == Some('\n') => Ok(()),
      Some(_) => Err(Box::new(self.error(self.here(), "expected the end of the line"))),
    }
  }

  /// `[name]`
  fn header(&mut self) -> Result<Table, Box<Diagnostic>> {
    let start = self.pos;

    self.bump();

    if self.peek() == Some('[') {
      self.bump();

      return Err(Box::new(self.unsupported(self.span(start), "arrays of tables (`[[name]]`)")));
    }

    self.skip_blank(false);

    let name = self.key()?;

    self.skip_blank(false);

    if self.peek() == Some('.') {
      return Err(Box::new(self.unsupported(self.here(), "dotted table names")));
    }

    if self.peek() != Some(']') {
      return Err(Box::new(self.error(self.here(), "expected `]` closing the table header")));
    }

    self.bump();

    let span = self.span(start);

    self.line_end()?;

    Ok(Table { name, span, entries: Vec::new() })
  }

  /// A bare key: ASCII letters, digits, `_` and `-`.
  fn key(&mut self) -> Result<String, Box<Diagnostic>> {
    let start = self.pos;

    while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
      self.bump();
    }

    if self.pos > start {
      return Ok(self.text.get(start..self.pos).unwrap_or_default().to_string());
    }

    match self.peek() {
      Some('"' | '\'') => Err(Box::new(self.unsupported(self.here(), "quoted keys"))),
      _ => Err(Box::new(self.error(self.here(), "expected a key"))),
    }
  }

  /// `key = value`
  fn entry(&mut self) -> Result<Entry, Box<Diagnostic>> {
    let start = self.pos;
    let key = self.key()?;
    let span = self.span(start);

    self.skip_blank(false);

    if self.peek() == Some('.') {
      return Err(Box::new(self.unsupported(self.here(), "dotted keys")));
    }

    if self.peek() != Some('=') {
      return Err(Box::new(self.error(self.here(), format!("expected `=` after the key `{key}`"))));
    }

    self.bump();
    self.skip_blank(false);

    let value = self.value()?;

    self.line_end()?;

    Ok(Entry { key, span, value })
  }

  fn value(&mut self) -> Result<Value, Box<Diagnostic>> {
    let start = self.pos;
    let kind = match self.peek() {
      Some('"') => ValueKind::String(self.basic_string()?),
      Some('\'') => ValueKind::String(self.literal_string()?),
      Some('[') => ValueKind::Array(self.array()?),
      Some('{') => ValueKind::Table(self.inline_table()?),
      Some(c) if c.is_ascii_alphanumeric() || c == '+' || c == '-' => self.word()?,
      _ => return Err(Box::new(self.error(self.here(), "expected a value: a string, an integer, `true`, `false` or an array"))),
    };

    Ok(Value { kind, span: self.span(start) })
  }

  /// `"text"`, with escapes.
  fn basic_string(&mut self) -> Result<String, Box<Diagnostic>> {
    let start = self.pos;

    self.bump();

    if self.peek() == Some('"') && self.peek_at(1) == Some('"') {
      self.bump();
      self.bump();

      return Err(Box::new(self.unsupported(self.span(start), "multi-line strings")));
    }

    let mut text = String::new();

    loop {
      match self.peek() {
        None | Some('\n' | '\r') => return Err(Box::new(self.error(self.span(start), "unterminated string"))),
        Some('"') => {
          self.bump();

          return Ok(text);
        }
        Some('\\') => {
          let escape = self.pos;

          self.bump();

          let decoded = match self.bump() {
            Some('"') => '"',
            Some('\\') => '\\',
            Some('n') => '\n',
            Some('t') => '\t',
            Some('r') => '\r',
            Some('b') => '\u{8}',
            Some('f') => '\u{c}',
            Some('u') => self.unicode_escape(escape, 4)?,
            Some('U') => self.unicode_escape(escape, 8)?,
            _ => return Err(Box::new(self.error(self.span(escape), "invalid escape sequence"))),
          };

          text.push(decoded);
        }
        Some(c) => {
          self.bump();
          text.push(c);
        }
      }
    }
  }

  /// The `digits` hexadecimal digits of `\uXXXX` or `\UXXXXXXXX`.
  fn unicode_escape(&mut self, escape: usize, digits: usize) -> Result<char, Box<Diagnostic>> {
    let mut value: u32 = 0;

    for _ in 0..digits {
      let digit = self.peek().and_then(|c| c.to_digit(16));
      let Some(digit) = digit else {
        return Err(Box::new(self.error(self.span(escape), format!("expected {digits} hexadecimal digits in the escape"))));
      };

      self.bump();
      value = value * 16 + digit;
    }

    char::from_u32(value).ok_or_else(|| Box::new(self.error(self.span(escape), "the escape is not a Unicode scalar value")))
  }

  /// `'text'`, without escapes.
  fn literal_string(&mut self) -> Result<String, Box<Diagnostic>> {
    let start = self.pos;

    self.bump();

    if self.peek() == Some('\'') && self.peek_at(1) == Some('\'') {
      self.bump();
      self.bump();

      return Err(Box::new(self.unsupported(self.span(start), "multi-line strings")));
    }

    let mut text = String::new();

    loop {
      match self.bump() {
        None | Some('\n' | '\r') => return Err(Box::new(self.error(self.span(start), "unterminated string"))),
        Some('\'') => return Ok(text),
        Some(c) => text.push(c),
      }
    }
  }

  /// `[value, ...]`, over several lines if needed, with an optional trailing comma.
  fn array(&mut self) -> Result<Vec<Value>, Box<Diagnostic>> {
    let start = self.pos;
    let mut values = Vec::new();

    self.bump();

    loop {
      self.skip_blank(true);

      match self.peek() {
        Some(']') => break,
        None => return Err(Box::new(self.error(self.span(start), "unterminated array"))),
        _ => {}
      }

      values.push(self.value()?);
      self.skip_blank(true);

      match self.peek() {
        Some(',') => {
          self.bump();
        }
        Some(']') => break,
        None => return Err(Box::new(self.error(self.span(start), "unterminated array"))),
        Some(_) => return Err(Box::new(self.error(self.here(), "expected `,` or `]` in the array"))),
      }
    }

    self.bump();

    Ok(values)
  }

  /// `{ key = value, ... }` on one line, without a trailing comma.
  fn inline_table(&mut self) -> Result<Vec<Entry>, Box<Diagnostic>> {
    let start = self.pos;
    let mut entries: Vec<Entry> = Vec::new();

    self.bump();
    self.skip_blank(false);

    if self.peek() == Some('}') {
      self.bump();

      return Ok(entries);
    }

    loop {
      self.skip_blank(false);

      let key_start = self.pos;
      let key = self.key()?;
      let span = self.span(key_start);

      self.skip_blank(false);

      if self.peek() != Some('=') {
        return Err(Box::new(self.error(self.here(), format!("expected `=` after the key `{key}`"))));
      }

      self.bump();
      self.skip_blank(false);

      let value = self.value()?;

      if let Some(previous) = entries.iter().find(|previous| previous.key == key) {
        return Err(Box::new(
          Diagnostic::error(codes::DUPLICATE, span, format!("the key `{key}` is defined twice")).with_label(previous.span, "first defined here"),
        ));
      }

      entries.push(Entry { key, span, value });
      self.skip_blank(false);

      match self.peek() {
        Some(',') => {
          self.bump();
          self.skip_blank(false);

          if self.peek() == Some('}') {
            return Err(Box::new(self.error(self.here(), "an inline table cannot end with a comma")));
          }
        }
        Some('}') => {
          self.bump();

          return Ok(entries);
        }
        None | Some('\n' | '\r') => return Err(Box::new(self.error(self.span(start), "an inline table must close on the line it starts"))),
        Some(_) => return Err(Box::new(self.error(self.here(), "expected `,` or `}` in the inline table"))),
      }
    }
  }

  /// `true`, `false` or a decimal integer such as `-1_000`.
  fn word(&mut self) -> Result<ValueKind, Box<Diagnostic>> {
    let start = self.pos;

    while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-' | '.' | ':')) {
      self.bump();
    }

    let span = self.span(start);
    let word = self.text.get(start..self.pos).unwrap_or_default();

    match word {
      "true" => return Ok(ValueKind::Boolean(true)),
      "false" => return Ok(ValueKind::Boolean(false)),
      _ => {}
    }

    let digits = word.strip_prefix(['+', '-']).unwrap_or(word);

    if !digits.starts_with(|c: char| c.is_ascii_digit()) {
      return Err(Box::new(self.error(span, format!("expected a value, found `{word}`")).with_help("write text in quotes: `\"text\"`")));
    }

    if word.contains(['.', ':']) || digits.contains(['e', 'E']) && !digits.starts_with("0x") {
      return Err(Box::new(self.unsupported(span, "floats and dates")));
    }

    let underscores_valid = !digits.starts_with('_') && !digits.ends_with('_') && !digits.contains("__");
    let plain: String = digits.chars().filter(|c| *c != '_').collect();
    let leading_zero = plain.len() > 1 && plain.starts_with('0');

    if !underscores_valid || leading_zero || !plain.bytes().all(|byte| byte.is_ascii_digit()) {
      return Err(Box::new(self.error(span, format!("invalid integer `{word}`")).with_note("integers are decimal, without leading zeros")));
    }

    let negative = word.starts_with('-');
    let text = if negative { format!("-{plain}") } else { plain };

    text.parse::<i64>().map(ValueKind::Integer).map_err(|_| Box::new(self.error(span, format!("the integer `{word}` does not fit in 64 bits"))))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use sagitario_span::SourceMap;
  use std::path::PathBuf;

  fn parse_text(text: &str) -> Result<Document, Box<Diagnostic>> {
    let mut sources = SourceMap::new();
    let file = sources.add(PathBuf::from("Nova.toml"), text.to_string());

    parse(file, text)
  }

  /// `table.key=value` for each entry, with values written without spans.
  fn entries(text: &str) -> Vec<String> {
    let document = parse_text(text).unwrap();

    document
      .tables
      .iter()
      .flat_map(|table| table.entries.iter().map(move |entry| format!("{}.{}={}", table.name, entry.key, shape(&entry.value.kind))))
      .collect()
  }

  fn shape(kind: &ValueKind) -> String {
    match kind {
      ValueKind::String(text) => format!("{text:?}"),
      ValueKind::Integer(value) => value.to_string(),
      ValueKind::Boolean(value) => value.to_string(),
      ValueKind::Array(values) => format!("[{}]", values.iter().map(|value| shape(&value.kind)).collect::<Vec<_>>().join(",")),
      ValueKind::Table(entries) => {
        format!("{{{}}}", entries.iter().map(|entry| format!("{}={}", entry.key, shape(&entry.value.kind))).collect::<Vec<_>>().join(","))
      }
    }
  }

  #[test]
  fn parses_tables_and_values() {
    let text = "top = 1\n# comment\n[package]  # header comment\nname = \"hi\"\nok = true\nno=false\nn = -1_000\nlist = [\n  \"a\", # first\n  'b\\n',\n]\nempty = []\r\n";

    assert_eq!(
      entries(text),
      vec![
        ".top=1",
        "package.name=\"hi\"",
        "package.ok=true",
        "package.no=false",
        "package.n=-1000",
        "package.list=[\"a\",\"b\\\\n\"]",
        "package.empty=[]"
      ]
    );
  }

  #[test]
  fn decodes_escapes() {
    assert_eq!(entries("s = \"a\\\"b\\\\c\\td\\u00f1\\U0001F600\""), vec![format!(".s={:?}", "a\"b\\c\td\u{f1}\u{1F600}")]);
  }

  #[test]
  fn reports_errors_and_unsupported_features() {
    for text in [
      "a = ",
      "a = \"open",
      "a = 'open",
      "a = \"\\q\"",
      "a 1",
      "a = 1 b",
      "[package",
      "[[bin]]",
      "[a.b]",
      "a.b = 1",
      "\"a\" = 1",
      "a = { b = 1, }",
      "a = { b = 1",
      "a = { b 1 }",
      "a = 1.5",
      "a = 1979-05-27",
      "a = 012",
      "a = 1__0",
      "a = 99999999999999999999",
      "a = [1, 2",
      "a = [1 2]",
      "a = \"\"\"x\"\"\"",
      "a = text",
    ] {
      let error = parse_text(text).unwrap_err();

      assert_eq!(error.code, codes::SYNTAX, "{text}");
    }
  }

  #[test]
  fn parses_inline_tables() {
    assert_eq!(entries("a = { path = \"../x\", n = [1] }\nb = {}"), vec![".a={path=\"../x\",n=[1]}", ".b={}"]);
    assert_eq!(parse_text("a = { b = 1, b = 2 }").unwrap_err().code, codes::DUPLICATE);
  }

  #[test]
  fn reports_duplicates() {
    assert_eq!(parse_text("a = 1\na = 2").unwrap_err().code, codes::DUPLICATE);
    assert_eq!(parse_text("[x]\n[x]").unwrap_err().code, codes::DUPLICATE);
  }
}
