//! Lexer: turns source text into tokens (docs/spec/01-lexical.md). Hand-written so it can be
//! ported to Sagitario for self-hosting.

use sagitario_diagnostics::{Code, Diagnostic, Diagnostics, codes};
use sagitario_span::{FileId, Span};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
  pub kind: TokenKind,
  pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
  Ident(String),
  Keyword(Keyword),
  /// An integer literal; `_` separators removed and the base already applied.
  Int {
    value: u128,
    suffix: Option<String>,
  },
  /// A float literal; `text` has `_` separators removed.
  Float {
    text: String,
    suffix: Option<String>,
  },
  /// A plain string literal with escapes already processed.
  Str(String),
  /// A template string: literal text alternating with `${...}` interpolations.
  Template(Vec<TemplatePart>),
  Char(char),
  /// `///` (outer) or `//!` (inner) documentation.
  DocComment {
    inner: bool,
    text: String,
  },
  LParen,
  RParen,
  LBrace,
  RBrace,
  LBracket,
  RBracket,
  Comma,
  Semi,
  Colon,
  ColonColon,
  Dot,
  DotDot,
  DotDotEq,
  Arrow,
  FatArrow,
  Assign,
  EqEq,
  NotEq,
  Lt,
  Le,
  Gt,
  Ge,
  Plus,
  Minus,
  Star,
  Slash,
  Percent,
  PlusEq,
  MinusEq,
  StarEq,
  SlashEq,
  PercentEq,
  AndAnd,
  OrOr,
  /// `|`, separating the alternatives of a pattern.
  Pipe,
  Bang,
  At,
  Question,
  Eof,
}

impl TokenKind {
  /// How the token is named in diagnostics, e.g. "`;`" or "identifier `x`".
  pub fn describe(&self) -> String {
    match self {
      TokenKind::Ident(name) => format!("identifier `{name}`"),
      TokenKind::Keyword(keyword) => format!("keyword `{}`", keyword.as_str()),
      TokenKind::Int { .. } => "integer literal".to_string(),
      TokenKind::Float { .. } => "float literal".to_string(),
      TokenKind::Str(_) => "string literal".to_string(),
      TokenKind::Template(_) => "template string".to_string(),
      TokenKind::Char(_) => "character literal".to_string(),
      TokenKind::DocComment { .. } => "documentation comment".to_string(),
      TokenKind::Eof => "end of file".to_string(),
      other => format!("`{}`", other.punctuation()),
    }
  }

  fn punctuation(&self) -> &'static str {
    match self {
      TokenKind::LParen => "(",
      TokenKind::RParen => ")",
      TokenKind::LBrace => "{",
      TokenKind::RBrace => "}",
      TokenKind::LBracket => "[",
      TokenKind::RBracket => "]",
      TokenKind::Comma => ",",
      TokenKind::Semi => ";",
      TokenKind::Colon => ":",
      TokenKind::ColonColon => "::",
      TokenKind::Dot => ".",
      TokenKind::DotDot => "..",
      TokenKind::DotDotEq => "..=",
      TokenKind::Arrow => "->",
      TokenKind::FatArrow => "=>",
      TokenKind::Assign => "=",
      TokenKind::EqEq => "==",
      TokenKind::NotEq => "!=",
      TokenKind::Lt => "<",
      TokenKind::Le => "<=",
      TokenKind::Gt => ">",
      TokenKind::Ge => ">=",
      TokenKind::Plus => "+",
      TokenKind::Minus => "-",
      TokenKind::Star => "*",
      TokenKind::Slash => "/",
      TokenKind::Percent => "%",
      TokenKind::PlusEq => "+=",
      TokenKind::MinusEq => "-=",
      TokenKind::StarEq => "*=",
      TokenKind::SlashEq => "/=",
      TokenKind::PercentEq => "%=",
      TokenKind::AndAnd => "&&",
      TokenKind::OrOr => "||",
      TokenKind::Pipe => "|",
      TokenKind::Bang => "!",
      TokenKind::At => "@",
      TokenKind::Question => "?",
      _ => "?",
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplatePart {
  /// Literal text with escapes already processed.
  Text(String),
  /// The tokens of one `${...}` interpolation, ending with `Eof`.
  Expr(Vec<Token>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
  Abstract,
  As,
  Break,
  Class,
  Const,
  Continue,
  Else,
  Enum,
  Extends,
  Extern,
  False,
  Fn,
  For,
  If,
  Impl,
  In,
  Interface,
  /// Only in `if let` and `while let`.
  Let,
  Loop,
  Match,
  New,
  Override,
  Pkg,
  Prv,
  Pub,
  Return,
  SelfValue,
  SelfType,
  Struct,
  Super,
  True,
  Unsafe,
  Use,
  Var,
  Weak,
  Where,
  While,
  Zod,
  /// Reserved for the concurrency phase.
  Async,
  /// Reserved for the concurrency phase.
  Await,
}

const KEYWORDS: &[(&str, Keyword)] = &[
  ("abstract", Keyword::Abstract),
  ("as", Keyword::As),
  ("break", Keyword::Break),
  ("class", Keyword::Class),
  ("const", Keyword::Const),
  ("continue", Keyword::Continue),
  ("else", Keyword::Else),
  ("enum", Keyword::Enum),
  ("extends", Keyword::Extends),
  ("extern", Keyword::Extern),
  ("false", Keyword::False),
  ("fn", Keyword::Fn),
  ("for", Keyword::For),
  ("if", Keyword::If),
  ("impl", Keyword::Impl),
  ("in", Keyword::In),
  ("interface", Keyword::Interface),
  ("let", Keyword::Let),
  ("loop", Keyword::Loop),
  ("match", Keyword::Match),
  ("new", Keyword::New),
  ("override", Keyword::Override),
  ("pkg", Keyword::Pkg),
  ("prv", Keyword::Prv),
  ("pub", Keyword::Pub),
  ("return", Keyword::Return),
  ("self", Keyword::SelfValue),
  ("Self", Keyword::SelfType),
  ("struct", Keyword::Struct),
  ("super", Keyword::Super),
  ("true", Keyword::True),
  ("unsafe", Keyword::Unsafe),
  ("use", Keyword::Use),
  ("var", Keyword::Var),
  ("weak", Keyword::Weak),
  ("where", Keyword::Where),
  ("while", Keyword::While),
  ("zod", Keyword::Zod),
  ("async", Keyword::Async),
  ("await", Keyword::Await),
];

impl Keyword {
  pub fn from_ident(text: &str) -> Option<Keyword> {
    KEYWORDS.iter().find(|(name, _)| *name == text).map(|(_, keyword)| *keyword)
  }

  pub fn as_str(self) -> &'static str {
    KEYWORDS.iter().find(|(_, keyword)| *keyword == self).map_or("?", |(name, _)| name)
  }

  /// Keywords reserved for future phases; programs cannot use them yet.
  pub fn is_reserved(self) -> bool {
    matches!(self, Keyword::Async | Keyword::Await)
  }
}

pub const INT_SUFFIXES: &[&str] = &["i8", "i16", "i32", "i64", "isize", "u8", "u16", "u32", "u64", "usize"];
pub const FLOAT_SUFFIXES: &[&str] = &["f32", "f64"];

/// Lexes a whole file. The result always ends with an `Eof` token.
pub fn lex(file: FileId, text: &str, diagnostics: &mut Diagnostics) -> Vec<Token> {
  let mut lexer = Lexer { file, text, pos: 0, tokens: Vec::new(), diagnostics };

  lexer.run();

  lexer.tokens
}

struct Lexer<'a> {
  file: FileId,
  text: &'a str,
  pos: usize,
  tokens: Vec<Token>,
  diagnostics: &'a mut Diagnostics,
}

impl Lexer<'_> {
  fn run(&mut self) {
    while let Some(c) = self.peek() {
      self.step(c);
    }

    let end = self.pos;

    self.push(TokenKind::Eof, end);
  }

  /// Lexes one token (or skips whitespace or a comment) starting with `c`.
  fn step(&mut self, c: char) {
    let start = self.pos;

    if c.is_whitespace() {
      self.bump();
    } else if c == '/' && self.peek_at(1) == Some('/') {
      self.line_comment(start);
    } else if c == '/' && self.peek_at(1) == Some('*') {
      self.block_comment(start);
    } else if c.is_ascii_digit() {
      self.number(start);
    } else if c == '_' || c.is_alphabetic() {
      self.ident(start);
    } else if c == '"' {
      self.string(start);
    } else if c == '\'' {
      self.char_literal(start);
    } else if c == '`' {
      self.template(start);
    } else {
      self.punctuation(start, c);
    }
  }

  fn peek(&self) -> Option<char> {
    self.text.get(self.pos..).and_then(|rest| rest.chars().next())
  }

  fn peek_at(&self, n: usize) -> Option<char> {
    self.text.get(self.pos..).and_then(|rest| rest.chars().nth(n))
  }

  fn bump(&mut self) -> Option<char> {
    let c = self.peek()?;
    self.pos += c.len_utf8();

    Some(c)
  }

  fn eat(&mut self, expected: char) -> bool {
    if self.peek() == Some(expected) {
      self.bump();

      return true;
    }

    false
  }

  fn span(&self, start: usize) -> Span {
    Span::new(self.file, start as u32, self.pos as u32)
  }

  fn push(&mut self, kind: TokenKind, start: usize) {
    let span = self.span(start);

    self.tokens.push(Token { kind, span });
  }

  fn error(&mut self, code: Code, start: usize, message: impl Into<String>) {
    let span = self.span(start);

    self.diagnostics.push(Diagnostic::error(code, span, message));
  }

  fn slice(&self, start: usize) -> &str {
    self.text.get(start..self.pos).unwrap_or("")
  }

  fn line_comment(&mut self, start: usize) {
    self.pos += 2;
    // `///` is an outer doc comment, but `////` is a plain comment (as in Rust).

    let doc = if self.peek() == Some('/') && self.peek_at(1) != Some('/') {
      Some(false)
    } else if self.peek() == Some('!') {
      Some(true)
    } else {
      None
    };

    if doc.is_some() {
      self.bump();
    }

    let text_start = self.pos;

    while self.peek().is_some_and(|c| c != '\n') {
      self.bump();
    }

    if let Some(inner) = doc {
      let raw = self.slice(text_start);
      let text = raw.strip_prefix(' ').unwrap_or(raw).trim_end_matches('\r').to_string();

      self.push(TokenKind::DocComment { inner, text }, start);
    }
  }

  fn block_comment(&mut self, start: usize) {
    self.pos += 2;

    let mut depth = 1;

    while depth > 0 {
      match (self.peek(), self.peek_at(1)) {
        (None, _) => {
          self.diagnostics.push(Diagnostic::error(
            codes::UNTERMINATED_BLOCK_COMMENT,
            Span::new(self.file, start as u32, start as u32 + 2),
            "unterminated block comment",
          ));

          return;
        }
        (Some('/'), Some('*')) => {
          self.pos += 2;
          depth += 1;
        }
        (Some('*'), Some('/')) => {
          self.pos += 2;
          depth -= 1;
        }
        _ => {
          self.bump();
        }
      }
    }
  }

  fn ident(&mut self, start: usize) {
    while self.peek().is_some_and(|c| c == '_' || c.is_alphanumeric()) {
      self.bump();
    }

    let text = self.slice(start);
    let kind = match Keyword::from_ident(text) {
      Some(keyword) => TokenKind::Keyword(keyword),
      None => TokenKind::Ident(text.to_string()),
    };

    self.push(kind, start);
  }

  fn number(&mut self, start: usize) {
    let radix = match (self.peek(), self.peek_at(1)) {
      (Some('0'), Some('x')) => 16,
      (Some('0'), Some('o')) => 8,
      (Some('0'), Some('b')) => 2,
      _ => 10,
    };

    if radix != 10 {
      self.pos += 2;
    }

    let mut valid = true;
    let mut text = self.digit_run(radix, &mut valid);
    let mut is_float = false;

    if radix == 10 {
      if self.peek() == Some('.') && self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
        self.bump();
        text.push('.');
        text.push_str(&self.digit_run(10, &mut valid));

        is_float = true;
      }

      let exponent_follows = match (self.peek(), self.peek_at(1), self.peek_at(2)) {
        (Some('e' | 'E'), Some(digit), _) if digit.is_ascii_digit() => true,
        (Some('e' | 'E'), Some('+' | '-'), Some(digit)) => digit.is_ascii_digit(),
        _ => false,
      };

      if exponent_follows {
        self.bump();
        text.push('e');

        if let Some(sign @ ('+' | '-')) = self.peek() {
          self.bump();
          text.push(sign);
        }

        text.push_str(&self.digit_run(10, &mut valid));
        is_float = true;
      }
    }

    let suffix_start = self.pos;

    while self.peek().is_some_and(|c| c == '_' || c.is_alphanumeric()) {
      self.bump();
    }

    let suffix = Some(self.slice(suffix_start).to_string()).filter(|suffix| !suffix.is_empty());

    if let Some(suffix) = &suffix {
      if FLOAT_SUFFIXES.contains(&suffix.as_str()) {
        if radix != 10 {
          self.error(codes::INVALID_NUMBER, start, format!("a base {radix} literal cannot have the float suffix `{suffix}`"));
          valid = false;
        }

        is_float = true;
      } else if !INT_SUFFIXES.contains(&suffix.as_str()) || is_float {
        self.error(codes::INVALID_NUMBER, start, format!("invalid suffix `{suffix}` for a numeric literal"));
        valid = false;
      }
    }

    if !valid {
      self.push(TokenKind::Int { value: 0, suffix: None }, start);
    } else if is_float {
      self.push(TokenKind::Float { text, suffix }, start);
    } else {
      match u128::from_str_radix(&text, radix) {
        Ok(value) => self.push(TokenKind::Int { value, suffix }, start),
        Err(_) => {
          self.error(codes::INVALID_NUMBER, start, "integer literal is too large");
          self.push(TokenKind::Int { value: 0, suffix: None }, start);
        }
      }
    }
  }

  /// Consumes digits and `_` separators, returning the digits. Reports misplaced separators and
  /// digits that do not belong to `radix`.
  fn digit_run(&mut self, radix: u32, valid: &mut bool) -> String {
    let start = self.pos;
    let mut digits = String::new();
    let mut last = None;

    while let Some(c) = self.peek() {
      let is_digit = c.is_ascii_digit() || (radix == 16 && c.is_ascii_hexdigit());

      if !is_digit && c != '_' {
        break;
      }

      self.bump();
      last = Some(c);

      if c != '_' {
        if !c.is_digit(radix) && *valid {
          self.error(codes::INVALID_NUMBER, start, format!("invalid digit `{c}` in a base {radix} literal"));
          *valid = false;
        }

        digits.push(c);
      }
    }

    let starts_with_separator = self.text.get(start..).is_some_and(|rest| rest.starts_with('_'));

    if *valid && (digits.is_empty() || starts_with_separator || last == Some('_')) {
      let message = if digits.is_empty() { "expected digits" } else { "`_` must appear between digits" };

      self.error(codes::INVALID_NUMBER, start, message);

      *valid = false;
    }

    digits
  }

  fn string(&mut self, start: usize) {
    self.bump();

    let mut value = String::new();

    loop {
      match self.peek() {
        None => {
          self.error(codes::UNTERMINATED_STRING, start, "unterminated string literal");
          break;
        }
        Some('"') => {
          self.bump();
          break;
        }
        Some('\\') => {
          if let Some(c) = self.escape() {
            value.push(c);
          }
        }
        Some(c) => {
          self.bump();
          value.push(c);
        }
      }
    }

    self.push(TokenKind::Str(value), start);
  }

  /// Consumes an escape sequence starting at `\`. Reports and returns `None` if it is invalid.
  fn escape(&mut self) -> Option<char> {
    let start = self.pos;

    self.bump();

    let c = self.bump();
    let value = match c {
      Some('n') => Some('\n'),
      Some('r') => Some('\r'),
      Some('t') => Some('\t'),
      Some('0') => Some('\0'),
      Some(c @ ('\\' | '"' | '\'' | '`' | '$')) => Some(c),
      Some('x') => {
        let digits: String = [self.bump(), self.bump()].into_iter().flatten().collect();

        u8::from_str_radix(&digits, 16).ok().filter(|byte| digits.len() == 2 && *byte <= 0x7F).map(char::from)
      }
      Some('u') if self.eat('{') => {
        let mut digits = String::new();

        while let Some(c) = self.peek().filter(|c| c.is_ascii_hexdigit()) {
          self.bump();
          digits.push(c);
        }

        let closed = self.eat('}');

        u32::from_str_radix(&digits, 16).ok().filter(|_| closed && digits.len() <= 6).and_then(char::from_u32)
      }
      _ => None,
    };
    if value.is_none() {
      let text = self.slice(start).to_string();

      self.error(codes::INVALID_ESCAPE, start, format!("invalid escape sequence `{text}`"));
    }
    value
  }

  fn char_literal(&mut self, start: usize) {
    self.bump();

    let value = match self.peek() {
      Some('\\') => self.escape(),
      Some('\'') | Some('\n') | None => {
        self.eat('\'');
        self.error(codes::INVALID_CHAR_LITERAL, start, "empty or unterminated character literal");
        self.push(TokenKind::Char('\0'), start);

        return;
      }
      Some(c) => {
        self.bump();
        Some(c)
      }
    };

    if !self.eat('\'') {
      while self.peek().is_some_and(|c| c != '\'' && c != '\n') {
        self.bump();
      }
      let closed = self.eat('\'');
      let message = if closed { "a character literal must contain exactly one character" } else { "unterminated character literal" };

      self.error(codes::INVALID_CHAR_LITERAL, start, message);
    }

    self.push(TokenKind::Char(value.unwrap_or('\0')), start);
  }

  fn template(&mut self, start: usize) {
    self.bump();

    let mut parts = Vec::new();
    let mut text = String::new();

    loop {
      match self.peek() {
        None => {
          self.error(codes::UNTERMINATED_STRING, start, "unterminated template string");

          break;
        }
        Some('`') => {
          self.bump();

          break;
        }
        Some('\\') => {
          if let Some(c) = self.escape() {
            text.push(c);
          }
        }
        Some('$') if self.peek_at(1) == Some('{') => {
          self.pos += 2;

          if !text.is_empty() {
            parts.push(TemplatePart::Text(std::mem::take(&mut text)));
          }

          parts.push(TemplatePart::Expr(self.interpolation(start)));
        }
        Some(c) => {
          self.bump();
          text.push(c);
        }
      }
    }

    if !text.is_empty() {
      parts.push(TemplatePart::Text(text));
    }

    self.push(TokenKind::Template(parts), start);
  }

  /// Lexes the tokens of a `${...}` interpolation up to its closing `}`, which is consumed.
  fn interpolation(&mut self, template_start: usize) -> Vec<Token> {
    let outer = std::mem::take(&mut self.tokens);
    let mut depth = 0usize;

    loop {
      let Some(c) = self.peek() else {
        self.error(codes::UNTERMINATED_STRING, template_start, "unterminated interpolation in template string");

        break;
      };

      if c == '}' && depth == 0 {
        self.bump();

        break;
      }

      self.step(c);

      match self.tokens.last().map(|token| &token.kind) {
        Some(TokenKind::LBrace) => depth += 1,
        Some(TokenKind::RBrace) => depth = depth.saturating_sub(1),
        _ => {}
      }
    }

    let end = self.pos;

    self.push(TokenKind::Eof, end);

    std::mem::replace(&mut self.tokens, outer)
  }

  fn punctuation(&mut self, start: usize, c: char) {
    self.bump();

    let kind = match c {
      '(' => TokenKind::LParen,
      ')' => TokenKind::RParen,
      '{' => TokenKind::LBrace,
      '}' => TokenKind::RBrace,
      '[' => TokenKind::LBracket,
      ']' => TokenKind::RBracket,
      ',' => TokenKind::Comma,
      ';' => TokenKind::Semi,
      '@' => TokenKind::At,
      '?' => TokenKind::Question,
      ':' if self.eat(':') => TokenKind::ColonColon,
      ':' => TokenKind::Colon,
      '.' if self.eat('.') => {
        if self.eat('=') {
          TokenKind::DotDotEq
        } else {
          TokenKind::DotDot
        }
      }
      '.' => TokenKind::Dot,
      '-' if self.eat('>') => TokenKind::Arrow,
      '-' if self.eat('=') => TokenKind::MinusEq,
      '-' => TokenKind::Minus,
      '=' if self.eat('=') => TokenKind::EqEq,
      '=' if self.eat('>') => TokenKind::FatArrow,
      '=' => TokenKind::Assign,
      '!' if self.eat('=') => TokenKind::NotEq,
      '!' => TokenKind::Bang,
      '<' if self.eat('=') => TokenKind::Le,
      '<' => TokenKind::Lt,
      '>' if self.eat('=') => TokenKind::Ge,
      '>' => TokenKind::Gt,
      '+' if self.eat('=') => TokenKind::PlusEq,
      '+' => TokenKind::Plus,
      '*' if self.eat('=') => TokenKind::StarEq,
      '*' => TokenKind::Star,
      '/' if self.eat('=') => TokenKind::SlashEq,
      '/' => TokenKind::Slash,
      '%' if self.eat('=') => TokenKind::PercentEq,
      '%' => TokenKind::Percent,
      '&' if self.eat('&') => TokenKind::AndAnd,
      '|' if self.eat('|') => TokenKind::OrOr,
      '|' => TokenKind::Pipe,
      other => {
        self.error(codes::UNEXPECTED_CHARACTER, start, format!("unexpected character `{other}`"));

        return;
      }
    };

    self.push(kind, start);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use sagitario_span::SourceMap;
  use std::path::PathBuf;

  fn lex_text(text: &str) -> (Vec<TokenKind>, Vec<Diagnostic>) {
    let mut sources = SourceMap::new();
    let file = sources.add(PathBuf::from("test.sag"), text.to_string());
    let mut diagnostics = Diagnostics::new();
    let tokens = lex(file, text, &mut diagnostics);

    (tokens.into_iter().map(|token| token.kind).collect(), diagnostics.take())
  }

  fn kinds(text: &str) -> Vec<TokenKind> {
    let (kinds, diagnostics) = lex_text(text);

    assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");

    kinds
  }

  fn error_codes(text: &str) -> Vec<&'static str> {
    lex_text(text).1.iter().map(|diagnostic| diagnostic.code.0).collect()
  }

  fn int(value: u128, suffix: Option<&str>) -> TokenKind {
    TokenKind::Int { value, suffix: suffix.map(str::to_string) }
  }

  fn float(text: &str, suffix: Option<&str>) -> TokenKind {
    TokenKind::Float { text: text.to_string(), suffix: suffix.map(str::to_string) }
  }

  #[test]
  fn lexes_a_hello_world() {
    assert_eq!(
      kinds("pub fn main() { println(\"hola\"); }"),
      vec![
        TokenKind::Keyword(Keyword::Pub),
        TokenKind::Keyword(Keyword::Fn),
        TokenKind::Ident("main".to_string()),
        TokenKind::LParen,
        TokenKind::RParen,
        TokenKind::LBrace,
        TokenKind::Ident("println".to_string()),
        TokenKind::LParen,
        TokenKind::Str("hola".to_string()),
        TokenKind::RParen,
        TokenKind::Semi,
        TokenKind::RBrace,
        TokenKind::Eof,
      ]
    );
  }

  #[test]
  fn distinguishes_keywords_and_identifiers() {
    assert_eq!(
      kinds("self Self super zod await selfish _x ñandú"),
      vec![
        TokenKind::Keyword(Keyword::SelfValue),
        TokenKind::Keyword(Keyword::SelfType),
        TokenKind::Keyword(Keyword::Super),
        TokenKind::Keyword(Keyword::Zod),
        TokenKind::Keyword(Keyword::Await),
        TokenKind::Ident("selfish".to_string()),
        TokenKind::Ident("_x".to_string()),
        TokenKind::Ident("ñandú".to_string()),
        TokenKind::Eof,
      ]
    );
  }

  #[test]
  fn lexes_numeric_literals() {
    assert_eq!(
      kinds("255 0xFF 0o377 0b1111_1111 1_000_000 255u8 10i64 3.14 1e10 2.5e-3 1.5f32 7f64"),
      vec![
        int(255, None),
        int(255, None),
        int(255, None),
        int(255, None),
        int(1_000_000, None),
        int(255, Some("u8")),
        int(10, Some("i64")),
        float("3.14", None),
        float("1e10", None),
        float("2.5e-3", None),
        float("1.5", Some("f32")),
        float("7", Some("f64")),
        TokenKind::Eof,
      ]
    );
  }

  #[test]
  fn keeps_ranges_and_method_calls_apart_from_floats() {
    assert_eq!(kinds("0..10")[..3], [int(0, None), TokenKind::DotDot, int(10, None)]);
    assert_eq!(kinds("1..=2")[1], TokenKind::DotDotEq);
    assert_eq!(kinds("5.len")[..3], [int(5, None), TokenKind::Dot, TokenKind::Ident("len".to_string())]);
  }

  #[test]
  fn rejects_malformed_numbers() {
    for text in ["1_", "0x", "0x_1", "0b102", "12abc", "1.5u8", "0b1f32", "340282366920938463463374607431768211456"] {
      assert_eq!(error_codes(text), vec!["E0005"], "{text}");
    }
  }

  #[test]
  fn processes_escapes() {
    assert_eq!(
      kinds(r#""a\n\t\\\"\'\`\$\0\x41\u{1F600}" '\n' 'ñ'"#)[..3],
      [TokenKind::Str("a\n\t\\\"'`$\0A\u{1F600}".to_string()), TokenKind::Char('\n'), TokenKind::Char('ñ')]
    );
  }

  #[test]
  fn rejects_invalid_escapes_and_chars() {
    assert_eq!(error_codes(r#""\q""#), vec!["E0004"]);
    assert_eq!(error_codes(r#""\x80""#), vec!["E0004"]);
    assert_eq!(error_codes(r#""\u{110000}""#), vec!["E0004"]);
    assert_eq!(error_codes("''"), vec!["E0006"]);
    assert_eq!(error_codes("'ab'"), vec!["E0006"]);
    assert_eq!(error_codes("'a"), vec!["E0006"]);
  }

  #[test]
  fn reports_unterminated_literals_and_comments() {
    assert_eq!(error_codes("\"abc"), vec!["E0002"]);
    assert_eq!(error_codes("/* /* */"), vec!["E0003"]);
  }

  #[test]
  fn skips_comments_and_keeps_doc_comments() {
    assert_eq!(
      kinds("// line\n/* a /* nested */ b */ //// not doc\n/// item doc\n//! module doc\nx"),
      vec![
        TokenKind::DocComment { inner: false, text: "item doc".to_string() },
        TokenKind::DocComment { inner: true, text: "module doc".to_string() },
        TokenKind::Ident("x".to_string()),
        TokenKind::Eof,
      ]
    );
  }

  #[test]
  fn lexes_operators() {
    assert_eq!(
      kinds(":: -> => == != <= >= += -= *= /= %= && || .. ..= @ ?"),
      vec![
        TokenKind::ColonColon,
        TokenKind::Arrow,
        TokenKind::FatArrow,
        TokenKind::EqEq,
        TokenKind::NotEq,
        TokenKind::Le,
        TokenKind::Ge,
        TokenKind::PlusEq,
        TokenKind::MinusEq,
        TokenKind::StarEq,
        TokenKind::SlashEq,
        TokenKind::PercentEq,
        TokenKind::AndAnd,
        TokenKind::OrOr,
        TokenKind::DotDot,
        TokenKind::DotDotEq,
        TokenKind::At,
        TokenKind::Question,
        TokenKind::Eof,
      ]
    );
  }

  #[test]
  fn reports_unexpected_characters() {
    assert_eq!(error_codes("a # b"), vec!["E0001"]);
  }

  #[test]
  fn lexes_template_strings() {
    let kinds = kinds("`a ${x + 1} \\${b} ${ { y } }${`in ${z}`}`");
    let [TokenKind::Template(parts), TokenKind::Eof] = kinds.as_slice() else { panic!("unexpected tokens: {kinds:?}") };
    let [TemplatePart::Text(a), TemplatePart::Expr(first), TemplatePart::Text(literal), TemplatePart::Expr(braces), TemplatePart::Expr(nested)] =
      parts.as_slice()
    else {
      panic!("unexpected parts: {parts:?}")
    };

    assert_eq!(a, "a ");
    assert_eq!(
      first.iter().map(|token| token.kind.clone()).collect::<Vec<_>>(),
      vec![TokenKind::Ident("x".to_string()), TokenKind::Plus, int(1, None), TokenKind::Eof]
    );
    assert_eq!(literal, " ${b} ");
    assert_eq!(braces.len(), 4);
    assert!(matches!(nested.first().map(|token| &token.kind), Some(TokenKind::Template(_))));
  }

  #[test]
  fn reports_unterminated_templates() {
    assert_eq!(error_codes("`abc"), vec!["E0002"]);
    assert_eq!(error_codes("`a ${x"), vec!["E0002", "E0002"]);
  }
}
