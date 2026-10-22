//! Parser: recursive descent from tokens to the AST. Errors are reported as diagnostics and the
//! parser recovers at item and statement boundaries, so one mistake does not hide the rest.

use sagitario_ast::{Block, Expr, ExprKind, FnDecl, Ident, Item, Module, NodeId, Param, Path, Stmt, TypeExpr, TypeExprKind, Visibility};
use sagitario_diagnostics::{Diagnostic, Diagnostics, codes};
use sagitario_lexer::{Keyword, Token, TokenKind};
use sagitario_span::{FileId, Span};

pub fn parse_module(file: FileId, tokens: Vec<Token>, diagnostics: &mut Diagnostics) -> Module {
  // Documentation comments are not processed in the MVP (docs/spec/01-lexical.md).
  let tokens: Vec<Token> = tokens.into_iter().filter(|token| !matches!(token.kind, TokenKind::DocComment { .. })).collect();
  let eof_span = tokens.last().map_or(Span::new(file, 0, 0), |token| token.span);
  let mut parser = Parser { tokens, pos: 0, eof: Token { kind: TokenKind::Eof, span: eof_span }, next_id: 0, diagnostics };

  parser.module(file)
}

/// Marker for a syntax error that has already been reported.
struct ParseError;

type PResult<T> = Result<T, ParseError>;

struct Parser<'a> {
  tokens: Vec<Token>,
  pos: usize,
  eof: Token,
  next_id: u32,
  diagnostics: &'a mut Diagnostics,
}

impl Parser<'_> {
  fn current(&self) -> &Token {
    self.tokens.get(self.pos).unwrap_or(&self.eof)
  }

  fn kind(&self) -> &TokenKind {
    &self.current().kind
  }

  fn nth_kind(&self, n: usize) -> &TokenKind {
    self.tokens.get(self.pos + n).map_or(&self.eof.kind, |token| &token.kind)
  }

  fn span(&self) -> Span {
    self.current().span
  }

  fn at(&self, kind: &TokenKind) -> bool {
    self.kind() == kind
  }

  fn advance(&mut self) -> Token {
    let token = self.current().clone();

    if self.pos < self.tokens.len() {
      self.pos += 1;
    }

    token
  }

  fn eat(&mut self, kind: &TokenKind) -> bool {
    if self.at(kind) {
      self.advance();

      return true;
    }

    false
  }

  fn expect(&mut self, kind: &TokenKind) -> PResult<Token> {
    if self.at(kind) {
      return Ok(self.advance());
    }

    Err(self.unexpected(&kind.describe()))
  }

  fn next_id(&mut self) -> NodeId {
    let id = NodeId(self.next_id);
    self.next_id += 1;

    id
  }

  fn unexpected(&mut self, expected: &str) -> ParseError {
    let found = self.kind().describe();
    let span = self.span();

    self.diagnostics.push(Diagnostic::error(codes::UNEXPECTED_TOKEN, span, format!("expected {expected}, found {found}")));

    ParseError
  }

  fn not_implemented(&mut self, span: Span, what: &str) -> ParseError {
    self.diagnostics.push(Diagnostic::error(codes::NOT_IMPLEMENTED, span, format!("{what} are not implemented yet")));

    ParseError
  }

  fn reserved_keyword(&mut self, keyword: Keyword) -> ParseError {
    let span = self.span();
    self.diagnostics.push(Diagnostic::error(
      codes::RESERVED_KEYWORD,
      span,
      format!("`{}` is reserved for a future version of Sagitario", keyword.as_str()),
    ));
    ParseError
  }

  fn module(&mut self, file: FileId) -> Module {
    let mut items = Vec::new();

    while !self.at(&TokenKind::Eof) {
      let start = self.pos;

      match self.item() {
        Ok(Some(item)) => items.push(item),
        Ok(None) => {}
        Err(ParseError) => self.skip_item(),
      }

      if self.pos == start {
        self.advance();
      }
    }

    Module { file, items }
  }

  /// Skips to the end of the current item: a `;` or a closing `}` at nesting depth zero.
  fn skip_item(&mut self) {
    let mut depth = 0usize;

    loop {
      match self.kind() {
        TokenKind::Eof => return,
        TokenKind::LBrace | TokenKind::LParen | TokenKind::LBracket => depth += 1,
        TokenKind::RParen | TokenKind::RBracket => depth = depth.saturating_sub(1),
        TokenKind::RBrace => {
          depth = depth.saturating_sub(1);

          if depth == 0 {
            self.advance();

            return;
          }
        }
        TokenKind::Semi if depth == 0 => {
          self.advance();

          return;
        }
        _ => {}
      }

      self.advance();
    }
  }

  fn item(&mut self) -> PResult<Option<Item>> {
    let start = self.span();
    let visibility = self.visibility();

    match self.kind().clone() {
      TokenKind::Keyword(Keyword::Fn) => Ok(Some(Item::Fn(self.fn_decl(visibility, start)?))),
      TokenKind::Keyword(
        keyword @ (Keyword::Struct
        | Keyword::Enum
        | Keyword::Class
        | Keyword::Abstract
        | Keyword::Interface
        | Keyword::Use
        | Keyword::Const
        | Keyword::Extern),
      ) => {
        let span = self.span();

        Err(self.not_implemented(span, &format!("`{}` items", keyword.as_str())))
      }
      TokenKind::At => {
        let span = self.span();

        Err(self.not_implemented(span, "attributes"))
      }
      _ => Err(self.unexpected("an item")),
    }
  }

  fn visibility(&mut self) -> Visibility {
    if !self.eat(&TokenKind::Keyword(Keyword::Pub)) {
      return Visibility::Module;
    }

    let is_package = self.at(&TokenKind::LParen) && *self.nth_kind(1) == TokenKind::Keyword(Keyword::Pkg) && *self.nth_kind(2) == TokenKind::RParen;

    if is_package {
      self.pos += 3;

      return Visibility::Package;
    }

    Visibility::Public
  }

  fn fn_decl(&mut self, visibility: Visibility, start: Span) -> PResult<FnDecl> {
    let id = self.next_id();
    self.expect(&TokenKind::Keyword(Keyword::Fn))?;

    let name = self.ident("a function name")?;
    self.expect(&TokenKind::LParen)?;

    let mut params = Vec::new();

    while !self.at(&TokenKind::RParen) {
      let param_name = self.ident("a parameter name")?;
      self.expect(&TokenKind::Colon)?;

      let ty = self.type_expr()?;
      params.push(Param { span: param_name.span.to(ty.span), name: param_name, ty });

      if !self.eat(&TokenKind::Comma) {
        break;
      }
    }

    self.expect(&TokenKind::RParen)?;

    let return_type = if self.eat(&TokenKind::Arrow) { Some(self.type_expr()?) } else { None };
    let body = self.block()?;

    Ok(FnDecl { id, visibility, name, params, return_type, span: start.to(body.span), body })
  }

  fn ident(&mut self, expected: &str) -> PResult<Ident> {
    match self.kind().clone() {
      TokenKind::Ident(name) => {
        let span = self.advance().span;

        Ok(Ident { name, span })
      }
      TokenKind::Keyword(keyword) if keyword.is_reserved() => Err(self.reserved_keyword(keyword)),
      _ => Err(self.unexpected(expected)),
    }
  }

  fn type_expr(&mut self) -> PResult<TypeExpr> {
    let start = self.span();

    match self.kind() {
      TokenKind::LParen if *self.nth_kind(1) == TokenKind::RParen => {
        self.advance();

        let end = self.advance().span;

        Ok(TypeExpr { kind: TypeExprKind::Unit, span: start.to(end) })
      }
      TokenKind::Ident(_) => {
        let path = self.path()?;

        if self.at(&TokenKind::Lt) {
          let span = self.span();

          return Err(self.not_implemented(span, "generic types"));
        }

        Ok(TypeExpr { span: path.span, kind: TypeExprKind::Path(path) })
      }
      TokenKind::LBracket => Err(self.not_implemented(start, "array types")),
      TokenKind::Keyword(Keyword::SelfType) => Err(self.not_implemented(start, "`Self` types")),
      _ => Err(self.unexpected("a type")),
    }
  }

  fn path(&mut self) -> PResult<Path> {
    let first = self.ident("a name")?;
    let mut span = first.span;
    let mut segments = vec![first];

    while self.at(&TokenKind::ColonColon) && matches!(self.nth_kind(1), TokenKind::Ident(_)) {
      self.advance();

      let segment = self.ident("a name")?;

      span = span.to(segment.span);
      segments.push(segment);
    }

    Ok(Path { segments, span })
  }

  fn block(&mut self) -> PResult<Block> {
    let start = self.expect(&TokenKind::LBrace)?.span;
    let mut stmts = Vec::new();
    let mut tail = None;

    loop {
      match self.kind() {
        TokenKind::RBrace => break,
        TokenKind::Eof => return Err(self.unexpected("`}`")),
        TokenKind::Keyword(
          keyword @ (Keyword::Const
          | Keyword::Var
          | Keyword::If
          | Keyword::While
          | Keyword::Loop
          | Keyword::For
          | Keyword::Return
          | Keyword::Match
          | Keyword::Break
          | Keyword::Continue
          | Keyword::Unsafe),
        ) => {
          let what = format!("`{}` statements", keyword.as_str());
          let span = self.span();

          self.not_implemented(span, &what);
          self.skip_stmt();

          continue;
        }
        _ => {}
      }
      let expr = match self.expr() {
        Ok(expr) => expr,
        Err(ParseError) => {
          self.skip_stmt();

          continue;
        }
      };
      if self.eat(&TokenKind::Semi) {
        stmts.push(Stmt::Expr(expr));
      } else if self.at(&TokenKind::RBrace) {
        tail = Some(Box::new(expr));
      } else {
        self.unexpected("`;` or `}`");
        self.skip_stmt();
      }
    }

    let end = self.advance().span;

    Ok(Block { stmts, tail, span: start.to(end) })
  }

  /// Skips to just after the next `;`, or to the `}` closing the current block, at depth zero.
  fn skip_stmt(&mut self) {
    let start = self.pos;
    let mut depth = 0usize;

    loop {
      match self.kind() {
        TokenKind::Eof => return,
        TokenKind::LBrace | TokenKind::LParen | TokenKind::LBracket => depth += 1,
        TokenKind::RParen | TokenKind::RBracket => depth = depth.saturating_sub(1),
        TokenKind::RBrace if depth == 0 => {
          // A stray `}` right where the statement started would stop progress; skip it.
          if self.pos == start {
            self.advance();
          }

          return;
        }
        TokenKind::RBrace => depth -= 1,
        TokenKind::Semi if depth == 0 => {
          self.advance();

          return;
        }
        _ => {}
      }

      self.advance();
    }
  }

  fn expr(&mut self) -> PResult<Expr> {
    let expr = self.postfix_expr()?;

    if is_binary_operator(self.kind()) {
      let span = self.span();

      return Err(self.not_implemented(span, "operators"));
    }

    Ok(expr)
  }

  fn postfix_expr(&mut self) -> PResult<Expr> {
    let mut expr = self.primary_expr()?;

    loop {
      match self.kind() {
        TokenKind::LParen => {
          self.advance();

          let mut args = Vec::new();

          while !self.at(&TokenKind::RParen) {
            args.push(self.expr()?);

            if !self.eat(&TokenKind::Comma) {
              break;
            }
          }

          let end = self.expect(&TokenKind::RParen)?.span;
          let span = expr.span.to(end);

          expr = Expr { id: self.next_id(), kind: ExprKind::Call { callee: Box::new(expr), args }, span };
        }
        TokenKind::Dot => {
          let span = self.span();

          return Err(self.not_implemented(span, "methods and fields"));
        }
        TokenKind::Question => {
          let span = self.span();

          return Err(self.not_implemented(span, "`?` operators"));
        }
        TokenKind::LBracket => {
          let span = self.span();

          return Err(self.not_implemented(span, "index expressions"));
        }
        _ => return Ok(expr),
      }
    }
  }

  fn primary_expr(&mut self) -> PResult<Expr> {
    let span = self.span();
    let kind = match self.kind().clone() {
      TokenKind::Int { value, suffix } => {
        self.advance();

        ExprKind::Int { value, suffix }
      }
      TokenKind::Float { text, suffix } => {
        self.advance();

        ExprKind::Float { text, suffix }
      }
      TokenKind::Str(value) => {
        self.advance();

        ExprKind::Str(value)
      }
      TokenKind::Char(value) => {
        self.advance();

        ExprKind::Char(value)
      }
      TokenKind::Keyword(keyword @ (Keyword::True | Keyword::False)) => {
        self.advance();

        ExprKind::Bool(keyword == Keyword::True)
      }
      TokenKind::Ident(_) => {
        let path = self.path()?;

        return Ok(Expr { id: self.next_id(), span: path.span, kind: ExprKind::Path(path) });
      }
      TokenKind::LParen if *self.nth_kind(1) == TokenKind::RParen => {
        self.advance();

        let end = self.advance().span;

        return Ok(Expr { id: self.next_id(), kind: ExprKind::Unit, span: span.to(end) });
      }
      TokenKind::LParen => {
        self.advance();

        let inner = self.expr()?;

        self.expect(&TokenKind::RParen)?;

        return Ok(inner);
      }
      TokenKind::Keyword(keyword) if keyword.is_reserved() => return Err(self.reserved_keyword(keyword)),
      TokenKind::Keyword(Keyword::New) => return Err(self.not_implemented(span, "`new` expressions")),
      TokenKind::Keyword(Keyword::SelfValue | Keyword::Zod) => return Err(self.not_implemented(span, "methods")),
      TokenKind::Keyword(Keyword::If | Keyword::Match | Keyword::Loop | Keyword::Unsafe) => {
        return Err(self.not_implemented(span, "block expressions"));
      }
      TokenKind::LBracket => return Err(self.not_implemented(span, "array literals")),
      TokenKind::Minus | TokenKind::Bang => return Err(self.not_implemented(span, "operators")),
      _ => return Err(self.unexpected("an expression")),
    };

    Ok(Expr { id: self.next_id(), kind, span })
  }
}

fn is_binary_operator(kind: &TokenKind) -> bool {
  matches!(
    kind,
    TokenKind::Plus
      | TokenKind::Minus
      | TokenKind::Star
      | TokenKind::Slash
      | TokenKind::Percent
      | TokenKind::EqEq
      | TokenKind::NotEq
      | TokenKind::Lt
      | TokenKind::Le
      | TokenKind::Gt
      | TokenKind::Ge
      | TokenKind::AndAnd
      | TokenKind::OrOr
      | TokenKind::Assign
      | TokenKind::PlusEq
      | TokenKind::MinusEq
      | TokenKind::StarEq
      | TokenKind::SlashEq
      | TokenKind::PercentEq
      | TokenKind::DotDot
      | TokenKind::DotDotEq
      | TokenKind::Keyword(Keyword::As)
  )
}

#[cfg(test)]
mod tests {
  use super::*;
  use sagitario_span::SourceMap;
  use std::path::PathBuf;

  fn parse(text: &str) -> (Module, Vec<&'static str>) {
    let mut sources = SourceMap::new();
    let file = sources.add(PathBuf::from("test.sag"), text.to_string());
    let mut diagnostics = Diagnostics::new();
    let tokens = sagitario_lexer::lex(file, text, &mut diagnostics);
    let module = parse_module(file, tokens, &mut diagnostics);

    (module, diagnostics.take().iter().map(|diagnostic| diagnostic.code.0).collect())
  }

  fn only_fn(module: &Module) -> &FnDecl {
    let [Item::Fn(decl)] = module.items.as_slice() else { panic!("expected one function, got {:?}", module.items) };

    decl
  }

  #[test]
  fn parses_a_hello_world() {
    let (module, errors) = parse("/// Entry point.\npub fn main() { println(\"hola\"); }");
    assert!(errors.is_empty(), "{errors:?}");

    let main = only_fn(&module);
    assert_eq!(main.name.name, "main");
    assert_eq!(main.visibility, Visibility::Public);
    assert!(main.params.is_empty() && main.return_type.is_none());

    let [Stmt::Expr(call)] = main.body.stmts.as_slice() else { panic!("expected one statement") };
    let ExprKind::Call { callee, args } = &call.kind else { panic!("expected a call, got {call:?}") };
    assert!(matches!(&callee.kind, ExprKind::Path(path) if path.segments[0].name == "println"));
    assert!(matches!(args.as_slice(), [Expr { kind: ExprKind::Str(text), .. }] if text == "hola"));
  }

  #[test]
  fn parses_params_return_types_and_tail_expressions() {
    let (module, errors) = parse("pub(pkg) fn greet(name: String, times: u8,) -> () { show(name, 3,) }");
    assert!(errors.is_empty(), "{errors:?}");

    let greet = only_fn(&module);
    assert_eq!(greet.visibility, Visibility::Package);
    assert_eq!(greet.params.len(), 2);
    assert_eq!(greet.return_type.as_ref().map(|ty| &ty.kind), Some(&TypeExprKind::Unit));
    assert!(greet.body.stmts.is_empty());
    assert!(matches!(greet.body.tail.as_deref(), Some(Expr { kind: ExprKind::Call { args, .. }, .. }) if args.len() == 2));
  }

  #[test]
  fn parses_literals_and_paths() {
    let (module, errors) = parse("fn f() { 1u8; 2.5; 'c'; true; (); std::io::println; }");
    assert!(errors.is_empty(), "{errors:?}");

    let kinds: Vec<&ExprKind> = only_fn(&module).body.stmts.iter().map(|Stmt::Expr(expr)| &expr.kind).collect();
    assert!(matches!(kinds[0], ExprKind::Int { value: 1, suffix: Some(suffix) } if suffix == "u8"));
    assert!(matches!(kinds[1], ExprKind::Float { .. }));
    assert_eq!(kinds[2], &ExprKind::Char('c'));
    assert_eq!(kinds[3], &ExprKind::Bool(true));
    assert_eq!(kinds[4], &ExprKind::Unit);
    assert!(matches!(kinds[5], ExprKind::Path(path) if path.segments.len() == 3));
  }

  #[test]
  fn reports_and_recovers_from_syntax_errors() {
    let (module, errors) = parse("fn a() { println(\"x\") println(\"y\"); }\nfn b() { ; }\npub fn main() {}");
    assert_eq!(errors, vec!["E0100", "E0100"]);
    assert_eq!(module.items.len(), 3);
  }

  #[test]
  fn reports_reserved_keywords() {
    assert_eq!(parse("fn await() {}").1, vec!["E0101"]);
    assert_eq!(parse("fn f() { async; }").1, vec!["E0101"]);
  }

  #[test]
  fn reports_features_of_later_increments() {
    assert_eq!(parse("struct P {}\npub fn main() {}").1, vec!["E0900"]);
    assert_eq!(parse("fn f() { const x = 1; }").1, vec!["E0900"]);
    assert_eq!(parse("fn f() { 1 + 2; }").1, vec!["E0900"]);
  }

  #[test]
  fn reports_unclosed_blocks() {
    assert_eq!(parse("fn f() { println(\"x\");").1, vec!["E0100"]);
  }
}
