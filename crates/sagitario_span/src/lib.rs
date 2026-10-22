//! Source files and positions. Every token, syntax node and diagnostic points back to the source
//! through a `Span`.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(u32);

impl FileId {
  /// The position of the file in its `SourceMap`.
  pub fn index(self) -> usize {
    self.0 as usize
  }
}

/// A byte range `[start, end)` inside one source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
  pub file: FileId,
  pub start: u32,
  pub end: u32,
}

impl Span {
  pub fn new(file: FileId, start: u32, end: u32) -> Span {
    Span { file, start, end }
  }

  /// The smallest span covering `self` and `other`, which must be in the same file.
  pub fn to(self, other: Span) -> Span {
    Span { file: self.file, start: self.start.min(other.start), end: self.end.max(other.end) }
  }
}

/// A 1-based position. Columns count Unicode scalar values, not bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineCol {
  pub line: u32,
  pub column: u32,
}

#[derive(Debug)]
pub struct SourceFile {
  path: PathBuf,
  text: String,
  /// Byte offset where each line starts.
  line_starts: Vec<u32>,
}

impl SourceFile {
  fn new(path: PathBuf, text: String) -> SourceFile {
    let mut line_starts = vec![0];

    for (offset, byte) in text.bytes().enumerate() {
      if byte == b'\n' {
        line_starts.push(offset as u32 + 1);
      }
    }

    SourceFile { path, text, line_starts }
  }

  pub fn path(&self) -> &Path {
    &self.path
  }

  pub fn text(&self) -> &str {
    &self.text
  }

  pub fn line_col(&self, offset: u32) -> LineCol {
    let line_index = match self.line_starts.binary_search(&offset) {
      Ok(index) => index,
      Err(index) => index.saturating_sub(1),
    };
    let line_start = self.line_starts.get(line_index).copied().unwrap_or(0);
    let column = self.text.get(line_start as usize..offset as usize).map_or(0, |prefix| prefix.chars().count());

    LineCol { line: line_index as u32 + 1, column: column as u32 + 1 }
  }

  /// The text of a 1-based line, without its line terminator.
  pub fn line_text(&self, line: u32) -> &str {
    let index = (line as usize).saturating_sub(1);
    let Some(&start) = self.line_starts.get(index) else {
      return "";
    };
    let end = self.line_starts.get(index + 1).map_or(self.text.len(), |&end| end as usize);

    self.text.get(start as usize..end).unwrap_or("").trim_end_matches(['\n', '\r'])
  }

  pub fn slice(&self, span: Span) -> &str {
    self.text.get(span.start as usize..span.end as usize).unwrap_or("")
  }
}

#[derive(Debug, Default)]
pub struct SourceMap {
  files: Vec<SourceFile>,
}

impl SourceMap {
  pub fn new() -> SourceMap {
    SourceMap::default()
  }

  /// Adds a file. Callers must reject texts longer than `u32::MAX` bytes.
  pub fn add(&mut self, path: PathBuf, text: String) -> FileId {
    let id = FileId(self.files.len() as u32);
    self.files.push(SourceFile::new(path, text));

    id
  }

  pub fn get(&self, id: FileId) -> Option<&SourceFile> {
    self.files.get(id.0 as usize)
  }

  /// Every file, in the order they were added.
  pub fn files(&self) -> &[SourceFile] {
    &self.files
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn file(text: &str) -> (SourceMap, FileId) {
    let mut sources = SourceMap::new();
    let id = sources.add(PathBuf::from("main.sag"), text.to_string());

    (sources, id)
  }

  #[test]
  fn computes_line_and_column() {
    let (sources, id) = file("ab\ncd\n\nefg");
    let file = sources.get(id).unwrap();

    assert_eq!(file.line_col(0), LineCol { line: 1, column: 1 });
    assert_eq!(file.line_col(1), LineCol { line: 1, column: 2 });
    assert_eq!(file.line_col(3), LineCol { line: 2, column: 1 });
    assert_eq!(file.line_col(6), LineCol { line: 3, column: 1 });
    assert_eq!(file.line_col(9), LineCol { line: 4, column: 3 });
  }

  #[test]
  fn columns_count_characters_not_bytes() {
    let (sources, id) = file("ñé x");

    assert_eq!(sources.get(id).unwrap().line_col(5), LineCol { line: 1, column: 4 });
  }

  #[test]
  fn returns_line_text_without_terminator() {
    let (sources, id) = file("first\r\nsecond\nthird");
    let file = sources.get(id).unwrap();

    assert_eq!(file.line_text(1), "first");
    assert_eq!(file.line_text(2), "second");
    assert_eq!(file.line_text(3), "third");
    assert_eq!(file.line_text(4), "");
  }

  #[test]
  fn joins_spans() {
    let (_, id) = file("abcdef");

    assert_eq!(Span::new(id, 1, 2).to(Span::new(id, 4, 6)), Span::new(id, 1, 6));
  }
}
