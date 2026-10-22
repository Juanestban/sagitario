//! Parser: recursive descent from tokens to the AST, with Pratt parsing for binary operators
//! (precedence table in docs/spec/06-control-flow.md#operators). Errors are reported as
//! diagnostics and the parser recovers at item and statement boundaries.

use sagitario_ast::{
  ArrayLen, BinaryOp, Block, EnumDecl, Expr, ExprKind, FieldDecl, FieldInit, FieldPattern, FnDecl, Ident, Item, LetStmt, MatchArm, Module, NodeId,
  Param, Path, Pattern, PatternKind, SelfParam, Stmt, StructDecl, TemplatePart, TypeExpr, TypeExprKind, UnaryOp, VariantDecl, VariantFieldDecl,
  VariantFields, Visibility,
};
use sagitario_diagnostics::{Diagnostic, Diagnostics, codes};
use sagitario_lexer::{self as lexer, Keyword, Token, TokenKind};
use sagitario_span::{FileId, Span};

pub fn parse_module(file: FileId, tokens: Vec<Token>, diagnostics: &mut Diagnostics) -> Module {
  let mut next_id = 0;
  let mut parser = Parser::new(file, tokens, &mut next_id, diagnostics);

  parser.module(file)
}

/// Marker for a syntax error that has already been reported.
struct ParseError;

type PResult<T> = Result<T, ParseError>;

/// What an infix token does, with its binding power (higher binds tighter).
#[derive(Clone, Copy)]
enum Infix {
  Binary(BinaryOp),
  Range { inclusive: bool },
  As,
}

const PREC_RANGE: u8 = 1;
const PREC_OR: u8 = 2;
const PREC_AND: u8 = 3;
const PREC_COMPARE: u8 = 4;
const PREC_ADD: u8 = 5;
const PREC_MUL: u8 = 6;
const PREC_AS: u8 = 7;

fn infix(kind: &TokenKind) -> Option<(u8, Infix)> {
  let binary = |prec, op| Some((prec, Infix::Binary(op)));

  match kind {
    TokenKind::DotDot => Some((PREC_RANGE, Infix::Range { inclusive: false })),
    TokenKind::DotDotEq => Some((PREC_RANGE, Infix::Range { inclusive: true })),
    TokenKind::OrOr => binary(PREC_OR, BinaryOp::Or),
    TokenKind::AndAnd => binary(PREC_AND, BinaryOp::And),
    TokenKind::EqEq => binary(PREC_COMPARE, BinaryOp::Eq),
    TokenKind::NotEq => binary(PREC_COMPARE, BinaryOp::Ne),
    TokenKind::Lt => binary(PREC_COMPARE, BinaryOp::Lt),
    TokenKind::Le => binary(PREC_COMPARE, BinaryOp::Le),
    TokenKind::Gt => binary(PREC_COMPARE, BinaryOp::Gt),
    TokenKind::Ge => binary(PREC_COMPARE, BinaryOp::Ge),
    TokenKind::Plus => binary(PREC_ADD, BinaryOp::Add),
    TokenKind::Minus => binary(PREC_ADD, BinaryOp::Sub),
    TokenKind::Star => binary(PREC_MUL, BinaryOp::Mul),
    TokenKind::Slash => binary(PREC_MUL, BinaryOp::Div),
    TokenKind::Percent => binary(PREC_MUL, BinaryOp::Rem),
    TokenKind::Keyword(Keyword::As) => Some((PREC_AS, Infix::As)),
    _ => None,
  }
}

fn assignment_op(kind: &TokenKind) -> Option<Option<BinaryOp>> {
  match kind {
    TokenKind::Assign => Some(None),
    TokenKind::PlusEq => Some(Some(BinaryOp::Add)),
    TokenKind::MinusEq => Some(Some(BinaryOp::Sub)),
    TokenKind::StarEq => Some(Some(BinaryOp::Mul)),
    TokenKind::SlashEq => Some(Some(BinaryOp::Div)),
    TokenKind::PercentEq => Some(Some(BinaryOp::Rem)),
    _ => None,
  }
}

/// Tokens that end an expression, so `break`/`return` before them carry no value.
fn ends_expression(kind: &TokenKind) -> bool {
  matches!(kind, TokenKind::Semi | TokenKind::RBrace | TokenKind::RParen | TokenKind::RBracket | TokenKind::Comma | TokenKind::Eof)
}

enum Member {
  Field(FieldDecl),
  Fn(FnDecl),
}

struct Parser<'a> {
  tokens: Vec<Token>,
  pos: usize,
  eof: Token,
  next_id: &'a mut u32,
  diagnostics: &'a mut Diagnostics,
  /// Set in the condition of `if`/`while` and the iterable of `for`, where `{` starts the body:
  /// `if x == Point { ... }` is not a struct literal there (as in Rust).
  no_struct_literal: bool,
}

impl<'a> Parser<'a> {
  fn new(file: FileId, tokens: Vec<Token>, next_id: &'a mut u32, diagnostics: &'a mut Diagnostics) -> Parser<'a> {
    // Documentation comments are not processed in the MVP (docs/spec/01-lexical.md).
    let tokens: Vec<Token> = tokens.into_iter().filter(|token| !matches!(token.kind, TokenKind::DocComment { .. })).collect();
    let eof_span = tokens.last().map_or(Span::new(file, 0, 0), |token| token.span);

    Parser { tokens, pos: 0, eof: Token { kind: TokenKind::Eof, span: eof_span }, next_id, diagnostics, no_struct_literal: false }
  }

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
    let id = NodeId(*self.next_id);

    *self.next_id += 1;

    id
  }

  fn expr_node(&mut self, kind: ExprKind, span: Span) -> Expr {
    Expr { id: self.next_id(), kind, span }
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
      TokenKind::Keyword(Keyword::Fn) => Ok(Some(Item::Fn(self.fn_decl(visibility, start, false)?))),
      TokenKind::Keyword(Keyword::Struct) => Ok(Some(Item::Struct(self.struct_decl(visibility, start)?))),
      TokenKind::Keyword(Keyword::Enum) => Ok(Some(Item::Enum(self.enum_decl(visibility, start)?))),
      TokenKind::Keyword(keyword @ (Keyword::Class | Keyword::Abstract | Keyword::Interface | Keyword::Use | Keyword::Const | Keyword::Extern)) => {
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

  fn struct_decl(&mut self, visibility: Visibility, start: Span) -> PResult<StructDecl> {
    self.expect(&TokenKind::Keyword(Keyword::Struct))?;

    let name = self.ident("a struct name")?;

    if self.at(&TokenKind::Lt) {
      let span = self.span();

      return Err(self.not_implemented(span, "generic structs"));
    }

    if self.at(&TokenKind::Keyword(Keyword::Impl)) {
      let span = self.span();

      return Err(self.not_implemented(span, "interfaces on structs"));
    }

    self.expect(&TokenKind::LBrace)?;

    let mut fields = Vec::new();
    let mut functions = Vec::new();

    while !self.at(&TokenKind::RBrace) && !self.at(&TokenKind::Eof) {
      let before = self.pos;

      match self.member() {
        Ok(Member::Field(field)) => fields.push(field),
        Ok(Member::Fn(function)) => functions.push(function),
        Err(ParseError) => self.skip_member(),
      }

      if self.pos == before {
        self.advance();
      }
    }

    let end = self.expect(&TokenKind::RBrace)?.span;

    Ok(StructDecl { visibility, name, fields, functions, span: start.to(end) })
  }

  fn enum_decl(&mut self, visibility: Visibility, start: Span) -> PResult<EnumDecl> {
    self.expect(&TokenKind::Keyword(Keyword::Enum))?;

    let name = self.ident("an enum name")?;

    if self.at(&TokenKind::Lt) {
      let span = self.span();

      return Err(self.not_implemented(span, "generic enums"));
    }

    if self.at(&TokenKind::Keyword(Keyword::Impl)) {
      let span = self.span();

      return Err(self.not_implemented(span, "interfaces on enums"));
    }

    self.expect(&TokenKind::LBrace)?;

    let mut variants = Vec::new();

    // Variants come first, separated by commas.
    while matches!(self.kind(), TokenKind::Ident(_)) {
      match self.variant() {
        Ok(variant) => variants.push(variant),
        Err(ParseError) => self.skip_variant(),
      }

      if !self.eat(&TokenKind::Comma) {
        break;
      }
    }

    if variants.is_empty() {
      self.diagnostics.push(
        Diagnostic::error(codes::EMPTY_DECLARATION, name.span, format!("the enum `{}` has no variants", name.name))
          .with_help("declare at least one variant: `enum Name { First, Second }`"),
      );
    }

    let mut functions = Vec::new();

    while !self.at(&TokenKind::RBrace) && !self.at(&TokenKind::Eof) {
      let before = self.pos;

      match self.member() {
        Ok(Member::Fn(function)) => functions.push(function),
        Ok(Member::Field(field)) => {
          self.diagnostics.push(
            Diagnostic::error(codes::UNEXPECTED_TOKEN, field.span, "enums cannot have fields")
              .with_help("store data in the variants: `Variant(Type)` or `Variant { name: Type }`"),
          );
        }
        Err(ParseError) => self.skip_member(),
      }

      if self.pos == before {
        self.advance();
      }
    }

    let end = self.expect(&TokenKind::RBrace)?.span;

    Ok(EnumDecl { visibility, name, variants, functions, span: start.to(end) })
  }

  /// `Name`, `Name(T1, T2)` or `Name { a: T1, b: T2 }`.
  fn variant(&mut self) -> PResult<VariantDecl> {
    let name = self.ident("a variant name")?;
    let empty = |parser: &mut Self, span: Span, name: &Ident| {
      parser.diagnostics.push(
        Diagnostic::error(codes::EMPTY_DECLARATION, span, format!("the variant `{}` declares no data", name.name))
          .with_help(format!("write a variant without data as `{}`", name.name)),
      );
    };

    if self.eat(&TokenKind::LParen) {
      let mut types = Vec::new();

      while !self.at(&TokenKind::RParen) {
        types.push(self.type_expr()?);

        if !self.eat(&TokenKind::Comma) {
          break;
        }
      }

      let end = self.expect(&TokenKind::RParen)?.span;

      if types.is_empty() {
        empty(self, name.span.to(end), &name);
      }

      return Ok(VariantDecl { span: name.span.to(end), name, fields: VariantFields::Tuple(types) });
    }

    if self.eat(&TokenKind::LBrace) {
      let mut fields = Vec::new();

      while !self.at(&TokenKind::RBrace) {
        let field_name = self.ident("a field name")?;

        self.expect(&TokenKind::Colon)?;

        let ty = self.type_expr()?;

        fields.push(VariantFieldDecl { span: field_name.span.to(ty.span), name: field_name, ty });

        if !self.eat(&TokenKind::Comma) {
          break;
        }
      }

      let end = self.expect(&TokenKind::RBrace)?.span;

      if fields.is_empty() {
        empty(self, name.span.to(end), &name);
      }

      return Ok(VariantDecl { span: name.span.to(end), name, fields: VariantFields::Named(fields) });
    }

    Ok(VariantDecl { span: name.span, name, fields: VariantFields::Unit })
  }

  /// Skips a malformed variant: to the next `,` or the `}` closing the enum, at depth zero.
  fn skip_variant(&mut self) {
    let mut depth = 0usize;

    loop {
      match self.kind() {
        TokenKind::Eof => return,
        TokenKind::LBrace | TokenKind::LParen => depth += 1,
        TokenKind::RParen => depth = depth.saturating_sub(1),
        TokenKind::RBrace if depth == 0 => return,
        TokenKind::RBrace => depth -= 1,
        TokenKind::Comma if depth == 0 => return,
        _ => {}
      }

      self.advance();
    }
  }

  /// A field `pub name: Type;` or a function, both with a mandatory `pub` or `prv`.
  fn member(&mut self) -> PResult<Member> {
    let start = self.span();

    if self.at(&TokenKind::At) {
      return Err(self.not_implemented(start, "attributes"));
    }

    let visibility = match self.kind() {
      TokenKind::Keyword(Keyword::Pub) => {
        self.advance();

        Visibility::Public
      }
      TokenKind::Keyword(Keyword::Prv) => {
        self.advance();

        Visibility::Private
      }
      _ => {
        self.diagnostics.push(
          Diagnostic::error(codes::MISSING_VISIBILITY, start, "struct members must be declared `pub` or `prv`")
            .with_help("add `pub` to make it visible outside the struct, or `prv` to keep it private"),
        );

        Visibility::Public
      }
    };

    if self.at(&TokenKind::Keyword(Keyword::Fn)) {
      return Ok(Member::Fn(self.fn_decl(visibility, start, true)?));
    }

    let name = self.ident("a field name or `fn`")?;

    self.expect(&TokenKind::Colon)?;

    let ty = self.type_expr()?;
    let end = self.expect(&TokenKind::Semi)?.span;

    Ok(Member::Field(FieldDecl { visibility, name, ty, span: start.to(end) }))
  }

  /// Skips a malformed struct member: to just after a `;` or a `{ ... }` body at depth zero, or
  /// to the `}` closing the struct.
  fn skip_member(&mut self) {
    let mut depth = 0usize;

    loop {
      match self.kind() {
        TokenKind::Eof => return,
        TokenKind::LBrace => depth += 1,
        TokenKind::RBrace if depth == 0 => return,
        TokenKind::RBrace => {
          depth -= 1;

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

  fn fn_decl(&mut self, visibility: Visibility, start: Span, in_struct: bool) -> PResult<FnDecl> {
    let id = self.next_id();

    self.expect(&TokenKind::Keyword(Keyword::Fn))?;

    let name = if in_struct { self.member_name("a function name")? } else { self.ident("a function name")? };

    self.expect(&TokenKind::LParen)?;

    let mut self_param = None;
    let at_self = self.at(&TokenKind::Keyword(Keyword::SelfValue))
      || (self.at(&TokenKind::Keyword(Keyword::Var)) && *self.nth_kind(1) == TokenKind::Keyword(Keyword::SelfValue));

    if at_self {
      let self_start = self.span();
      let mutable = self.eat(&TokenKind::Keyword(Keyword::Var));
      let span = self_start.to(self.advance().span);

      if in_struct {
        self_param = Some(SelfParam { mutable, span });
      } else {
        self.diagnostics.push(
          Diagnostic::error(codes::UNEXPECTED_TOKEN, span, "a `self` parameter is only allowed in struct methods")
            .with_help("declare the function inside a `struct` to make it a method"),
        );
      }

      if !self.at(&TokenKind::RParen) {
        self.expect(&TokenKind::Comma)?;
      }
    }

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

    Ok(FnDecl { id, visibility, name, self_param, params, return_type, span: start.to(body.span), body })
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

  /// The name of a struct member, which may be `new` (the conventional constructor name,
  /// docs/spec/03-memory.md#static-functions).
  fn member_name(&mut self, expected: &str) -> PResult<Ident> {
    if self.at(&TokenKind::Keyword(Keyword::New)) {
      let span = self.advance().span;

      return Ok(Ident { name: "new".to_string(), span });
    }

    self.ident(expected)
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

        if self.eat(&TokenKind::Lt) {
          let mut args = Vec::new();

          loop {
            args.push(self.type_expr()?);

            if !self.eat(&TokenKind::Comma) {
              break;
            }
          }

          let end = self.expect(&TokenKind::Gt)?.span;

          return Ok(TypeExpr { span: path.span.to(end), kind: TypeExprKind::Generic { path, args } });
        }

        Ok(TypeExpr { span: path.span, kind: TypeExprKind::Path(path) })
      }
      TokenKind::LBracket => {
        self.advance();

        let elem = self.type_expr()?;

        self.expect(&TokenKind::Semi)?;

        let len = self.array_len()?;
        let end = self.expect(&TokenKind::RBracket)?.span;

        Ok(TypeExpr { kind: TypeExprKind::Array { elem: Box::new(elem), len }, span: start.to(end) })
      }
      TokenKind::Keyword(Keyword::SelfType) => {
        let path = self.path()?;

        Ok(TypeExpr { span: path.span, kind: TypeExprKind::Path(path) })
      }
      _ => Err(self.unexpected("a type")),
    }
  }

  /// The `N` of `[T; N]` and `[value; N]`: an integer literal in F1.
  fn array_len(&mut self) -> PResult<ArrayLen> {
    let span = self.span();

    match self.kind().clone() {
      TokenKind::Int { value, suffix } => {
        self.advance();

        Ok(ArrayLen { value, suffix, span })
      }
      _ => Err(self.not_implemented(span, "array lengths other than integer literals")),
    }
  }

  /// A path; its first segment may be `self` or `Self`.
  fn path(&mut self) -> PResult<Path> {
    let first = match self.kind() {
      TokenKind::Keyword(keyword @ (Keyword::SelfValue | Keyword::SelfType)) => {
        let name = keyword.as_str().to_string();
        let span = self.advance().span;

        Ident { name, span }
      }
      _ => self.ident("a name")?,
    };
    let mut span = first.span;
    let mut segments = vec![first];

    while self.at(&TokenKind::ColonColon) && matches!(self.nth_kind(1), TokenKind::Ident(_) | TokenKind::Keyword(Keyword::New)) {
      self.advance();

      let segment = self.member_name("a name")?;

      span = span.to(segment.span);
      segments.push(segment);
    }

    Ok(Path { segments, span })
  }

  fn block(&mut self) -> PResult<Block> {
    let saved = std::mem::replace(&mut self.no_struct_literal, false);
    let block = self.block_inner();

    self.no_struct_literal = saved;

    block
  }

  fn block_inner(&mut self) -> PResult<Block> {
    let start = self.expect(&TokenKind::LBrace)?.span;
    let mut stmts = Vec::new();
    let mut tail = None;

    loop {
      match self.kind() {
        TokenKind::RBrace => break,
        TokenKind::Eof => return Err(self.unexpected("`}`")),
        _ => {}
      }

      if let Some(expr) = tail.take() {
        // A block-like expression followed by more code is a statement, not the tail.
        stmts.push(Stmt::Expr(expr));
      }

      match self.stmt() {
        Ok(Some(StmtOrTail::Stmt(stmt))) => stmts.push(stmt),
        Ok(Some(StmtOrTail::Tail(expr))) => tail = Some(expr),
        Ok(None) => {}
        Err(ParseError) => self.skip_stmt(),
      }
    }

    let end = self.advance().span;

    Ok(Block { stmts, tail: tail.map(Box::new), span: start.to(end) })
  }

  fn stmt(&mut self) -> PResult<Option<StmtOrTail>> {
    match self.kind() {
      TokenKind::Keyword(Keyword::Const | Keyword::Var) => return self.let_stmt().map(|stmt| Some(StmtOrTail::Stmt(stmt))),
      TokenKind::Keyword(Keyword::Unsafe) => {
        let span = self.span();

        return Err(self.not_implemented(span, "`unsafe` blocks"));
      }
      TokenKind::Semi => {
        // An empty statement is harmless but almost always a typo.
        return Err(self.unexpected("a statement"));
      }
      _ => {}
    }

    let expr = self.expr()?;

    if let Some(op) = assignment_op(self.kind()) {
      self.advance();

      let value = self.expr()?;
      let end = self.expect(&TokenKind::Semi)?.span;

      return Ok(Some(StmtOrTail::Stmt(Stmt::Assign { span: expr.span.to(end), target: expr, op, value })));
    }

    if self.eat(&TokenKind::Semi) {
      return Ok(Some(StmtOrTail::Stmt(Stmt::Expr(expr))));
    }

    if self.at(&TokenKind::RBrace) || expr.is_block_like() {
      return Ok(Some(StmtOrTail::Tail(expr)));
    }

    Err(self.unexpected("`;` or `}`"))
  }

  fn let_stmt(&mut self) -> PResult<Stmt> {
    let start = self.span();
    let mutable = self.advance().kind == TokenKind::Keyword(Keyword::Var);
    let id = self.next_id();
    let name = self.ident("a binding name")?;
    let ty = if self.eat(&TokenKind::Colon) { Some(self.type_expr()?) } else { None };
    let init = if self.eat(&TokenKind::Assign) { Some(self.expr()?) } else { None };
    let end = self.expect(&TokenKind::Semi)?.span;

    Ok(Stmt::Let(LetStmt { id, mutable, name, ty, init, span: start.to(end) }))
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
    self.binary_expr(PREC_RANGE)
  }

  /// Parses an expression with struct literals allowed or not.
  fn expr_with_struct_literals(&mut self, allowed: bool) -> PResult<Expr> {
    let saved = std::mem::replace(&mut self.no_struct_literal, !allowed);
    let expr = self.expr();

    self.no_struct_literal = saved;

    expr
  }

  /// `(a, b, c)` after a callee or method name.
  fn call_args(&mut self) -> PResult<(Vec<Expr>, Span)> {
    self.expect(&TokenKind::LParen)?;

    let mut args = Vec::new();

    while !self.at(&TokenKind::RParen) {
      args.push(self.expr_with_struct_literals(true)?);

      if !self.eat(&TokenKind::Comma) {
        break;
      }
    }

    let end = self.expect(&TokenKind::RParen)?.span;

    Ok((args, end))
  }

  fn binary_expr(&mut self, min_prec: u8) -> PResult<Expr> {
    let mut lhs = self.unary_expr()?;

    while let Some((prec, op)) = infix(self.kind()) {
      if prec < min_prec {
        break;
      }

      let op_span = self.advance().span;

      lhs = match op {
        Infix::As => {
          let ty = self.type_expr()?;
          let span = lhs.span.to(ty.span);

          self.expr_node(ExprKind::Cast { expr: Box::new(lhs), ty }, span)
        }
        Infix::Binary(op) => {
          let rhs = self.binary_expr(prec + 1)?;
          let span = lhs.span.to(rhs.span);

          self.expr_node(ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) }, span)
        }
        Infix::Range { inclusive } => {
          if ends_expression(self.kind()) || self.at(&TokenKind::LBrace) {
            return Err(self.not_implemented(op_span, "open ranges"));
          }

          let end = self.binary_expr(prec + 1)?;
          let span = lhs.span.to(end.span);

          self.expr_node(ExprKind::Range { start: Box::new(lhs), end: Box::new(end), inclusive }, span)
        }
      };

      // Comparisons and ranges are not associative: `a < b < c` is an error.
      if (prec == PREC_COMPARE || prec == PREC_RANGE) && infix(self.kind()).is_some_and(|(next, _)| next == prec) {
        let span = self.span();

        self.diagnostics.push(Diagnostic::error(
          codes::CHAINED_OPERATORS,
          span,
          "comparison and range operators cannot be chained; use parentheses or `&&`",
        ));

        return Err(ParseError);
      }
    }

    Ok(lhs)
  }

  fn unary_expr(&mut self) -> PResult<Expr> {
    let op = match self.kind() {
      TokenKind::Minus => UnaryOp::Neg,
      TokenKind::Bang => UnaryOp::Not,
      _ => return self.postfix_expr(),
    };
    let start = self.advance().span;
    let operand = self.unary_expr()?;
    let span = start.to(operand.span);

    Ok(self.expr_node(ExprKind::Unary { op, operand: Box::new(operand) }, span))
  }

  fn postfix_expr(&mut self) -> PResult<Expr> {
    let mut expr = self.primary_expr()?;

    loop {
      match self.kind() {
        TokenKind::LParen => {
          let (args, end) = self.call_args()?;
          let span = expr.span.to(end);

          expr = self.expr_node(ExprKind::Call { callee: Box::new(expr), args }, span);
        }
        TokenKind::Dot => {
          self.advance();

          let name = self.member_name("a field or method name")?;

          if self.at(&TokenKind::LParen) {
            let (args, end) = self.call_args()?;
            let span = expr.span.to(end);

            expr = self.expr_node(ExprKind::MethodCall { receiver: Box::new(expr), method: name, args }, span);
          } else {
            let span = expr.span.to(name.span);

            expr = self.expr_node(ExprKind::Field { base: Box::new(expr), name }, span);
          }
        }
        TokenKind::LBracket => {
          self.advance();

          let index = self.expr_with_struct_literals(true)?;
          let end = self.expect(&TokenKind::RBracket)?.span;
          let span = expr.span.to(end);

          expr = self.expr_node(ExprKind::Index { base: Box::new(expr), index: Box::new(index) }, span);
        }
        TokenKind::Question => {
          let end = self.advance().span;
          let span = expr.span.to(end);

          expr = self.expr_node(ExprKind::Try(Box::new(expr)), span);
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
      TokenKind::Template(parts) => {
        self.advance();

        ExprKind::Template(self.template_parts(parts)?)
      }
      TokenKind::Keyword(keyword @ (Keyword::True | Keyword::False)) => {
        self.advance();

        ExprKind::Bool(keyword == Keyword::True)
      }
      TokenKind::Ident(_) | TokenKind::Keyword(Keyword::SelfValue | Keyword::SelfType) => {
        let path = self.path()?;

        if self.at(&TokenKind::LBrace) && !self.no_struct_literal {
          return self.struct_literal(path);
        }

        let span = path.span;

        return Ok(self.expr_node(ExprKind::Path(path), span));
      }
      TokenKind::LBracket => return self.array_literal(),
      TokenKind::LParen if *self.nth_kind(1) == TokenKind::RParen => {
        self.advance();

        let end = self.advance().span;

        return Ok(self.expr_node(ExprKind::Unit, span.to(end)));
      }
      TokenKind::LParen => {
        self.advance();

        let inner = self.expr_with_struct_literals(true)?;

        self.expect(&TokenKind::RParen)?;

        return Ok(inner);
      }
      TokenKind::LBrace => {
        let block = self.block()?;
        let span = block.span;

        return Ok(self.expr_node(ExprKind::Block(block), span));
      }
      TokenKind::Keyword(Keyword::If) => return self.if_expr(),
      TokenKind::Keyword(Keyword::While) => {
        self.advance();

        let cond = self.expr_with_struct_literals(false)?;
        let body = self.block()?;
        let span = span.to(body.span);

        return Ok(self.expr_node(ExprKind::While { cond: Box::new(cond), body }, span));
      }
      TokenKind::Keyword(Keyword::Loop) => {
        self.advance();

        let body = self.block()?;
        let span = span.to(body.span);

        return Ok(self.expr_node(ExprKind::Loop { body }, span));
      }
      TokenKind::Keyword(Keyword::For) => {
        self.advance();

        let var = self.ident("a loop variable")?;

        self.expect(&TokenKind::Keyword(Keyword::In))?;

        let iter = self.expr_with_struct_literals(false)?;
        let body = self.block()?;
        let span = span.to(body.span);

        return Ok(self.expr_node(ExprKind::For { var, iter: Box::new(iter), body }, span));
      }
      TokenKind::Keyword(Keyword::Break) => {
        self.advance();

        let value = self.optional_value()?;
        let span = value.as_ref().map_or(span, |value| span.to(value.span));

        return Ok(self.expr_node(ExprKind::Break { value: value.map(Box::new) }, span));
      }
      TokenKind::Keyword(Keyword::Continue) => {
        self.advance();

        ExprKind::Continue
      }
      TokenKind::Keyword(Keyword::Return) => {
        self.advance();

        let value = self.optional_value()?;
        let span = value.as_ref().map_or(span, |value| span.to(value.span));

        return Ok(self.expr_node(ExprKind::Return { value: value.map(Box::new) }, span));
      }
      TokenKind::Keyword(keyword) if keyword.is_reserved() => return Err(self.reserved_keyword(keyword)),
      TokenKind::Keyword(Keyword::New) => {
        self.advance();

        let path = self.path()?;

        if self.at(&TokenKind::Lt) {
          let span = self.span();

          return Err(self.not_implemented(span, "generic types"));
        }

        let (args, end) = self.call_args()?;

        return Ok(self.expr_node(ExprKind::New { path, args }, span.to(end)));
      }
      TokenKind::Keyword(Keyword::Zod) => return Err(self.not_implemented(span, "`zod` expressions")),
      TokenKind::Keyword(Keyword::Match) => return self.match_expr(),
      TokenKind::Keyword(Keyword::Unsafe) => return Err(self.not_implemented(span, "`unsafe` blocks")),
      _ => return Err(self.unexpected("an expression")),
    };

    Ok(self.expr_node(kind, span))
  }

  fn optional_value(&mut self) -> PResult<Option<Expr>> {
    if ends_expression(self.kind()) {
      return Ok(None);
    }

    self.expr().map(Some)
  }

  fn if_expr(&mut self) -> PResult<Expr> {
    let start = self.expect(&TokenKind::Keyword(Keyword::If))?.span;
    let cond = self.expr_with_struct_literals(false)?;
    let then_block = self.block()?;
    let mut span = start.to(then_block.span);
    let else_branch = if self.eat(&TokenKind::Keyword(Keyword::Else)) {
      let branch = if self.at(&TokenKind::Keyword(Keyword::If)) {
        self.if_expr()?
      } else {
        let block = self.block()?;
        let span = block.span;

        self.expr_node(ExprKind::Block(block), span)
      };

      span = span.to(branch.span);

      Some(Box::new(branch))
    } else {
      None
    };

    Ok(self.expr_node(ExprKind::If { cond: Box::new(cond), then_block, else_branch }, span))
  }

  /// `Name { a: 1, b }` after its path.
  fn struct_literal(&mut self, path: Path) -> PResult<Expr> {
    self.expect(&TokenKind::LBrace)?;

    let mut fields = Vec::new();

    while !self.at(&TokenKind::RBrace) {
      let name = self.ident("a field name")?;
      let value = if self.eat(&TokenKind::Colon) {
        self.expr_with_struct_literals(true)?
      } else {
        let span = name.span;

        self.expr_node(ExprKind::Path(Path { segments: vec![name.clone()], span }), span)
      };

      fields.push(FieldInit { span: name.span.to(value.span), name, value });

      if !self.eat(&TokenKind::Comma) {
        break;
      }
    }

    let end = self.expect(&TokenKind::RBrace)?.span;
    let span = path.span.to(end);

    Ok(self.expr_node(ExprKind::StructLit { path, fields }, span))
  }

  /// `match scrutinee { pattern if guard => body, ... }`
  fn match_expr(&mut self) -> PResult<Expr> {
    let start = self.expect(&TokenKind::Keyword(Keyword::Match))?.span;
    let scrutinee = self.expr_with_struct_literals(false)?;

    self.expect(&TokenKind::LBrace)?;

    let saved = std::mem::replace(&mut self.no_struct_literal, false);
    let mut arms = Vec::new();
    let parsed = self.match_arms(&mut arms);

    self.no_struct_literal = saved;

    // After a malformed arm (already reported), skip the rest of the `match`; the arms parsed so
    // far keep the tree usable.
    let end = match parsed {
      Ok(()) => self.expect(&TokenKind::RBrace)?.span,
      Err(ParseError) => self.skip_past_close_brace(),
    };

    Ok(self.expr_node(ExprKind::Match { scrutinee: Box::new(scrutinee), arms }, start.to(end)))
  }

  /// Skips to just after the `}` closing the current braces.
  fn skip_past_close_brace(&mut self) -> Span {
    let mut depth = 0usize;

    loop {
      match self.kind() {
        TokenKind::Eof => return self.span(),
        TokenKind::LBrace => depth += 1,
        TokenKind::RBrace if depth == 0 => return self.advance().span,
        TokenKind::RBrace => depth -= 1,
        _ => {}
      }

      self.advance();
    }
  }

  fn match_arms(&mut self, arms: &mut Vec<MatchArm>) -> PResult<()> {
    while !self.at(&TokenKind::RBrace) {
      let pattern = self.pattern()?;
      let guard = if self.eat(&TokenKind::Keyword(Keyword::If)) { Some(self.expr()?) } else { None };

      self.expect(&TokenKind::FatArrow)?;

      let body = self.expr()?;
      let span = pattern.span.to(body.span);
      // A comma separates arms; it is optional after a block-like body and before `}`.
      let separated = self.eat(&TokenKind::Comma) || self.at(&TokenKind::RBrace) || body.is_block_like();

      arms.push(MatchArm { pattern, guard, body, span });

      if !separated {
        return Err(self.unexpected("`,` or `}`"));
      }
    }

    Ok(())
  }

  /// A pattern, with alternatives separated by `|`.
  fn pattern(&mut self) -> PResult<Pattern> {
    let first = self.pattern_alternative()?;

    if !self.at(&TokenKind::Pipe) {
      return Ok(first);
    }

    let mut span = first.span;
    let mut alternatives = vec![first];

    while self.eat(&TokenKind::Pipe) {
      let alternative = self.pattern_alternative()?;

      span = span.to(alternative.span);
      alternatives.push(alternative);
    }

    Ok(Pattern { id: self.next_id(), kind: PatternKind::Or(alternatives), span })
  }

  fn pattern_alternative(&mut self) -> PResult<Pattern> {
    let start = self.span();
    let literal = match self.kind() {
      TokenKind::Int { .. }
      | TokenKind::Float { .. }
      | TokenKind::Str(_)
      | TokenKind::Char(_)
      | TokenKind::Keyword(Keyword::True | Keyword::False) => true,
      TokenKind::Minus => matches!(self.nth_kind(1), TokenKind::Int { .. } | TokenKind::Float { .. }),
      _ => false,
    };

    if literal {
      let expr = self.unary_expr()?;
      let span = expr.span;

      return Ok(Pattern { id: self.next_id(), kind: PatternKind::Literal(Box::new(expr)), span });
    }

    if !matches!(self.kind(), TokenKind::Ident(_) | TokenKind::Keyword(Keyword::SelfType)) {
      return Err(self.unexpected("a pattern"));
    }

    let path = self.path()?;

    if self.eat(&TokenKind::LParen) {
      let mut elems = Vec::new();

      while !self.at(&TokenKind::RParen) {
        elems.push(self.pattern()?);

        if !self.eat(&TokenKind::Comma) {
          break;
        }
      }

      let end = self.expect(&TokenKind::RParen)?.span;

      return Ok(Pattern { id: self.next_id(), kind: PatternKind::Tuple { path, elems }, span: start.to(end) });
    }

    if self.eat(&TokenKind::LBrace) {
      let mut fields = Vec::new();
      let mut rest = false;

      while !self.at(&TokenKind::RBrace) {
        if self.eat(&TokenKind::DotDot) {
          rest = true;

          break;
        }

        let name = self.ident("a field name or `..`")?;
        let pattern = if self.eat(&TokenKind::Colon) {
          self.pattern()?
        } else {
          Pattern { id: self.next_id(), kind: PatternKind::Ident(name.clone()), span: name.span }
        };

        fields.push(FieldPattern { span: name.span.to(pattern.span), name, pattern });

        if !self.eat(&TokenKind::Comma) {
          break;
        }
      }

      let end = self.expect(&TokenKind::RBrace)?.span;

      return Ok(Pattern { id: self.next_id(), kind: PatternKind::Named { path, fields, rest }, span: start.to(end) });
    }

    let span = path.span;
    let kind = match path.segments.as_slice() {
      [segment] if segment.name == "_" => PatternKind::Wildcard,
      [segment] if segment.name != "Self" => PatternKind::Ident(segment.clone()),
      _ => PatternKind::Path(path),
    };

    Ok(Pattern { id: self.next_id(), kind, span })
  }

  /// `[a, b, c]` or `[value; N]`.
  fn array_literal(&mut self) -> PResult<Expr> {
    let start = self.expect(&TokenKind::LBracket)?.span;

    if self.at(&TokenKind::RBracket) {
      let end = self.advance().span;

      return Ok(self.expr_node(ExprKind::ArrayLit(Vec::new()), start.to(end)));
    }

    let first = self.expr_with_struct_literals(true)?;

    if self.eat(&TokenKind::Semi) {
      let len = self.array_len()?;
      let end = self.expect(&TokenKind::RBracket)?.span;

      return Ok(self.expr_node(ExprKind::ArrayRepeat { value: Box::new(first), len }, start.to(end)));
    }

    let mut elems = vec![first];

    while self.eat(&TokenKind::Comma) {
      if self.at(&TokenKind::RBracket) {
        break;
      }

      elems.push(self.expr_with_struct_literals(true)?);
    }

    let end = self.expect(&TokenKind::RBracket)?.span;

    Ok(self.expr_node(ExprKind::ArrayLit(elems), start.to(end)))
  }

  /// Parses each `${...}` interpolation with a sub-parser that shares node ids and diagnostics.
  fn template_parts(&mut self, parts: Vec<lexer::TemplatePart>) -> PResult<Vec<TemplatePart>> {
    let mut result = Vec::new();

    for part in parts {
      match part {
        lexer::TemplatePart::Text(text) => result.push(TemplatePart::Text(text)),
        lexer::TemplatePart::Expr(tokens) => {
          let file = self.eof.span.file;
          let mut sub = Parser::new(file, tokens, self.next_id, self.diagnostics);

          if sub.at(&TokenKind::Eof) {
            return Err(sub.unexpected("an expression inside `${}`"));
          }

          let expr = sub.expr()?;

          if !sub.at(&TokenKind::Eof) {
            return Err(sub.unexpected("`}` closing the interpolation"));
          }

          result.push(TemplatePart::Expr(expr));
        }
      }
    }

    Ok(result)
  }
}

enum StmtOrTail {
  Stmt(Stmt),
  /// An expression without `;`: the block's value if `}` follows, otherwise a block-like
  /// expression statement.
  Tail(Expr),
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

  /// Parses `fn f() { <expr> }` and renders the tail expression with explicit parentheses.
  fn shape(expr: &str) -> String {
    let (module, errors) = parse(&format!("fn f() {{ {expr} }}"));

    assert!(errors.is_empty(), "{errors:?}");

    render(only_fn(&module).body.tail.as_deref().expect("a tail expression"))
  }

  fn render(expr: &Expr) -> String {
    match &expr.kind {
      ExprKind::Int { value, .. } => value.to_string(),
      ExprKind::Bool(value) => value.to_string(),
      ExprKind::Path(path) => path.segments.iter().map(|segment| segment.name.as_str()).collect::<Vec<_>>().join("::"),
      ExprKind::Unary { op: UnaryOp::Neg, operand } => format!("(-{})", render(operand)),
      ExprKind::Unary { op: UnaryOp::Not, operand } => format!("(!{})", render(operand)),
      ExprKind::Binary { op, lhs, rhs } => format!("({} {} {})", render(lhs), op.symbol(), render(rhs)),
      ExprKind::Cast { expr, .. } => format!("({} as _)", render(expr)),
      ExprKind::Range { start, end, inclusive } => format!("({}{}{})", render(start), if *inclusive { "..=" } else { ".." }, render(end)),
      ExprKind::Call { callee, args } => format!("{}({})", render(callee), args.iter().map(render).collect::<Vec<_>>().join(", ")),
      other => format!("{other:?}"),
    }
  }

  #[test]
  fn parses_a_hello_world() {
    let (module, errors) = parse("/// Entry point.\npub fn main() { println(\"hola\"); }");

    assert!(errors.is_empty(), "{errors:?}");

    let main = only_fn(&module);

    assert_eq!(main.name.name, "main");
    assert_eq!(main.visibility, Visibility::Public);

    let [Stmt::Expr(call)] = main.body.stmts.as_slice() else { panic!("expected one statement") };

    assert_eq!(render(call), "println(Str(\"hola\"))");
  }

  #[test]
  fn respects_precedence_and_associativity() {
    assert_eq!(shape("1 + 2 * 3 - 4"), "((1 + (2 * 3)) - 4)");
    assert_eq!(shape("-a * b"), "((-a) * b)");
    assert_eq!(shape("-x as u8 + 1"), "(((-x) as _) + 1)");
    assert_eq!(shape("a < b && c || !d"), "(((a < b) && c) || (!d))");
    assert_eq!(shape("a == b + 1"), "(a == (b + 1))");
    assert_eq!(shape("0..n + 1"), "(0..(n + 1))");
    assert_eq!(shape("f(1, 2) % 3"), "(f(1, 2) % 3)");
  }

  #[test]
  fn rejects_chained_comparisons_and_open_ranges() {
    assert_eq!(parse("fn f() { a < b < c; }").1, vec!["E0102"]);
    assert_eq!(parse("fn f() { 0..1..2; }").1, vec!["E0102"]);
    assert_eq!(parse("fn f() { for i in 0.. {} }").1, vec!["E0900"]);
  }

  #[test]
  fn parses_statements() {
    let (module, errors) = parse(
      "fn f() -> i32 {
        const a: i32 = 1;
        var b;
        b = 2;
        b += a;
        if a > 0 { g(); } else if a < 0 { h(); } else { }
        while b > 0 { b -= 1; }
        for i in 0..=10 { continue; }
        const v = loop { break 5; };
        return a;
      }",
    );

    assert!(errors.is_empty(), "{errors:?}");

    let stmts = &only_fn(&module).body.stmts;

    assert_eq!(stmts.len(), 9);
    assert!(matches!(&stmts[0], Stmt::Let(LetStmt { mutable: false, ty: Some(_), init: Some(_), .. })));
    assert!(matches!(&stmts[1], Stmt::Let(LetStmt { mutable: true, ty: None, init: None, .. })));
    assert!(matches!(&stmts[3], Stmt::Assign { op: Some(BinaryOp::Add), .. }));
    assert!(matches!(&stmts[4], Stmt::Expr(Expr { kind: ExprKind::If { else_branch: Some(_), .. }, .. })));
    assert!(matches!(&stmts[6], Stmt::Expr(Expr { kind: ExprKind::For { .. }, .. })));
  }

  #[test]
  fn block_like_tails_followed_by_code_become_statements() {
    let (module, errors) = parse("fn f() { if c { a(); } b() }");

    assert!(errors.is_empty(), "{errors:?}");

    let body = &only_fn(&module).body;

    assert_eq!(body.stmts.len(), 1);
    assert!(body.tail.is_some());
  }

  #[test]
  fn parses_template_interpolations() {
    let (module, errors) = parse("fn f() { `a ${x + 1} b` }");

    assert!(errors.is_empty(), "{errors:?}");

    let tail = only_fn(&module).body.tail.as_deref().expect("tail");
    let ExprKind::Template(parts) = &tail.kind else { panic!("expected a template") };

    assert_eq!(parts.len(), 3);
    assert!(matches!(&parts[1], TemplatePart::Expr(expr) if render(expr) == "(x + 1)"));
    assert_eq!(parse("fn f() { `${}`; }").1, vec!["E0100"]);
  }

  #[test]
  fn reports_and_recovers_from_syntax_errors() {
    let (module, errors) = parse("fn a() { println(\"x\") println(\"y\"); }\nfn b() { ; }\npub fn main() {}");

    assert_eq!(errors, vec!["E0100", "E0100"]);
    assert_eq!(module.items.len(), 3);
  }

  #[test]
  fn reports_reserved_keywords_and_later_features() {
    assert_eq!(parse("fn await() {}").1, vec!["E0101"]);
    assert_eq!(parse("fn f() { async; }").1, vec!["E0101"]);
    assert_eq!(parse("class C {}\npub fn main() {}").1, vec!["E0900"]);
    assert_eq!(parse("enum E<T> { A }").1, vec!["E0900"]);
    assert_eq!(parse("struct P<T> {}").1, vec!["E0900"]);
    assert_eq!(parse("struct P impl Show {}").1, vec!["E0900"]);
  }

  #[test]
  fn parses_structs_and_their_members() {
    let (module, errors) = parse(
      "pub struct Point {
        pub x: f64;
        prv y: [f64; 2];
        pub fn new() -> Self { Self { x: 0.0, y: [0.0; 2] } }
        pub fn move_by(var self, dx: f64) { self.x += dx; }
        prv fn len(self) -> f64 { self.x }
      }",
    );

    assert!(errors.is_empty(), "{errors:?}");

    let [Item::Struct(decl)] = module.items.as_slice() else { panic!("expected one struct") };

    assert_eq!(decl.fields.len(), 2);
    assert_eq!(decl.fields[1].visibility, Visibility::Private);
    assert!(matches!(decl.fields[1].ty.kind, TypeExprKind::Array { .. }));
    assert_eq!(decl.functions.len(), 3);
    assert_eq!(decl.functions[0].name.name, "new");
    assert_eq!(decl.functions[0].self_param, None);
    assert!(decl.functions[1].self_param.is_some_and(|param| param.mutable));
    assert!(decl.functions[2].self_param.is_some_and(|param| !param.mutable));
  }

  #[test]
  fn parses_member_access_and_array_expressions() {
    let (module, errors) = parse("fn f() { a.b[0].c(1).d }");

    assert!(errors.is_empty(), "{errors:?}");

    let tail = only_fn(&module).body.tail.as_deref().expect("tail");
    let ExprKind::Field { base, name } = &tail.kind else { panic!("expected a field, got {tail:?}") };

    assert_eq!(name.name, "d");
    assert!(matches!(&base.kind, ExprKind::MethodCall { method, args, .. } if method.name == "c" && args.len() == 1));
    assert!(matches!(shape_kind("[1, 2, 3,]"), ExprKind::ArrayLit(elems) if elems.len() == 3));
    assert!(matches!(shape_kind("[0; 16]"), ExprKind::ArrayRepeat { len, .. } if len.value == 16));
    assert!(matches!(shape_kind("P { x, y: 1 }"), ExprKind::StructLit { fields, .. } if fields.len() == 2));
    assert_eq!(parse("fn f() { [0; n]; }").1, vec!["E0900"]);
  }

  #[test]
  fn struct_literals_are_not_parsed_in_conditions() {
    let (module, errors) = parse("fn f() { if x == P { } while ok { } for i in xs { } if (P { a: 1 }).a == 1 { } }");

    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(only_fn(&module).body.stmts.len(), 3);
  }

  #[test]
  fn reports_struct_syntax_errors() {
    assert_eq!(parse("struct P { x: i32; }").1, vec!["E0103"]);
    assert_eq!(parse("fn f(self) {}").1, vec!["E0100"]);
    assert_eq!(parse("struct P { pub fn f(x: i32, self) {} }").1, vec!["E0100"]);
  }

  #[test]
  fn parses_enums_match_and_try() {
    let (module, errors) = parse(
      "enum Shape { Circle(f64), Rect { w: f64, h: f64 }, Empty, pub fn unit() -> Self { Self::Empty } }
      fn f(s: Shape) -> Result<Option<i32>, String> {
        match s {
          Shape::Circle(r) | Shape::Rect { w: r, .. } if r > 0.0 => {}
          Shape::Empty => g()?,
          -1 | 'a' | true => {}
          _ => {}
        }
      }",
    );

    assert!(errors.is_empty(), "{errors:?}");

    let [Item::Enum(decl), Item::Fn(function)] = module.items.as_slice() else { panic!("expected an enum and a function") };

    assert_eq!(decl.variants.len(), 3);
    assert!(matches!(&decl.variants[1].fields, VariantFields::Named(fields) if fields.len() == 2));
    assert_eq!(decl.functions.len(), 1);
    assert!(matches!(function.return_type.as_ref().map(|ty| &ty.kind), Some(TypeExprKind::Generic { args, .. }) if args.len() == 2));

    let Some(ExprKind::Match { arms, .. }) = function.body.tail.as_deref().map(|tail| &tail.kind) else { panic!("expected a match") };

    assert_eq!(arms.len(), 4);
    assert!(matches!(&arms[0].pattern.kind, PatternKind::Or(alternatives) if alternatives.len() == 2));
    assert!(arms[0].guard.is_some());
    assert!(matches!(arms[1].body.kind, ExprKind::Try(_)));
    assert!(matches!(arms[3].pattern.kind, PatternKind::Wildcard));
    assert_eq!(parse("fn f() { match x { A => 1 B => 2 } }").1, vec!["E0100"]);
  }

  fn shape_kind(expr: &str) -> ExprKind {
    let (module, errors) = parse(&format!("fn f() {{ {expr} }}"));

    assert!(errors.is_empty(), "{errors:?}");

    only_fn(&module).body.tail.as_deref().expect("a tail expression").kind.clone()
  }

  #[test]
  fn reports_unclosed_blocks() {
    assert_eq!(parse("fn f() { println(\"x\");").1, vec!["E0100"]);
  }
}
