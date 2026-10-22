//! Exhaustiveness and reachability of `match` arms, with the usefulness algorithm of rustc
//! ("Warnings for pattern matching", Maranget 2007). Patterns are first simplified to
//! constructors applied to sub-patterns; a pattern is *useful* against earlier rows if some
//! value matches it and none of them. A `match` is exhaustive when `_` is not useful after all
//! its unguarded arms; the value that proves it is not is reported as missing.

use crate::{Type, TypedModule};

/// A way of building a value: a variant (by index), or a literal of a type with infinitely many
/// values (only a `_` covers all of those).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Ctor {
  Variant(u32),
  Bool(bool),
  Int(i128),
  Char(char),
  Str(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Pat {
  Wild,
  Ctor(Ctor, Vec<Pat>),
  Or(Vec<Pat>),
}

pub(crate) struct Space<'a> {
  pub(crate) typed: &'a TypedModule,
}

impl Space<'_> {
  /// Every constructor of a type with finitely many, with its field types; `None` otherwise.
  fn ctors(&self, ty: &Type) -> Option<Vec<(Ctor, Vec<Type>)>> {
    if *ty == Type::Bool {
      return Some(vec![(Ctor::Bool(false), Vec::new()), (Ctor::Bool(true), Vec::new())]);
    }

    let variants = self.typed.variants(ty)?;

    Some(variants.iter().enumerate().map(|(index, variant)| (Ctor::Variant(index as u32), variant.field_types())).collect())
  }

  /// Field types of `ctor`, padded with errors so they always number `arity`.
  fn fields(&self, ty: &Type, ctor: &Ctor, arity: usize) -> Vec<Type> {
    let mut fields = match ctor {
      Ctor::Variant(index) => self.typed.variants(ty).and_then(|variants| variants.get(*index as usize).map(|variant| variant.field_types())),
      _ => None,
    }
    .unwrap_or_default();

    fields.resize(arity, Type::Error);

    fields
  }

  /// A value matched by `row` and by none of `rows`, as one pattern per column; `None` if
  /// `row` is useless.
  pub(crate) fn useful(&self, rows: &[Vec<Pat>], row: &[Pat], types: &[Type]) -> Option<Vec<Pat>> {
    let Some((head, rest)) = row.split_first() else {
      return if rows.is_empty() { Some(Vec::new()) } else { None };
    };
    let ty = types.first().cloned().unwrap_or(Type::Error);
    let rest_types = types.get(1..).unwrap_or_default();

    match head {
      Pat::Or(alternatives) => alternatives.iter().find_map(|alternative| {
        let row: Vec<Pat> = std::iter::once(alternative.clone()).chain(rest.iter().cloned()).collect();

        self.useful(rows, &row, types)
      }),
      Pat::Ctor(ctor, args) => {
        let fields = self.fields(&ty, ctor, args.len());
        let row: Vec<Pat> = args.iter().chain(rest).cloned().collect();
        let types: Vec<Type> = fields.into_iter().chain(rest_types.iter().cloned()).collect();
        let witness = self.useful(&specialize(rows, ctor, args.len()), &row, &types)?;

        Some(rebuild(ctor, args.len(), witness))
      }
      Pat::Wild => {
        let used = head_ctors(rows);
        let all = self.ctors(&ty);

        if let Some(all) = all.as_ref().filter(|all| !all.is_empty() && all.iter().all(|(ctor, _)| used.contains(ctor))) {
          // Every constructor appears: `_` is useful if it is useful for one of them.
          return all.iter().find_map(|(ctor, fields)| {
            let row: Vec<Pat> = std::iter::repeat_n(Pat::Wild, fields.len()).chain(rest.iter().cloned()).collect();
            let types: Vec<Type> = fields.iter().cloned().chain(rest_types.iter().cloned()).collect();
            let witness = self.useful(&specialize(rows, ctor, fields.len()), &row, &types)?;

            Some(rebuild(ctor, fields.len(), witness))
          });
        }

        // Some constructor is missing: only rows starting with `_` can cover it.
        let witness = self.useful(&default_rows(rows), rest, rest_types)?;
        let missing = all.into_iter().flatten().find(|(ctor, _)| !used.contains(ctor));
        let head = match missing {
          Some((ctor, fields)) => Pat::Ctor(ctor, vec![Pat::Wild; fields.len()]),
          None => Pat::Wild,
        };

        Some(std::iter::once(head).chain(witness).collect())
      }
    }
  }
}

/// Constructors at the head of the rows, looking inside `|` patterns.
fn head_ctors(rows: &[Vec<Pat>]) -> Vec<Ctor> {
  fn collect(pat: &Pat, out: &mut Vec<Ctor>) {
    match pat {
      Pat::Ctor(ctor, _) if !out.contains(ctor) => out.push(ctor.clone()),
      Pat::Or(alternatives) => alternatives.iter().for_each(|alternative| collect(alternative, out)),
      _ => {}
    }
  }

  let mut out = Vec::new();

  for row in rows {
    if let Some(head) = row.first() {
      collect(head, &mut out);
    }
  }

  out
}

/// Rows with `|` heads split into one row per alternative.
fn expand(rows: &[Vec<Pat>]) -> Vec<Vec<Pat>> {
  let mut out = Vec::new();

  for row in rows {
    match row.split_first() {
      Some((Pat::Or(alternatives), rest)) => {
        let split: Vec<Vec<Pat>> =
          alternatives.iter().map(|alternative| std::iter::once(alternative.clone()).chain(rest.iter().cloned()).collect()).collect();

        out.extend(expand(&split));
      }
      _ => out.push(row.clone()),
    }
  }

  out
}

/// The rows that match values built with `ctor`, with its fields in place of the head.
fn specialize(rows: &[Vec<Pat>], ctor: &Ctor, arity: usize) -> Vec<Vec<Pat>> {
  expand(rows)
    .into_iter()
    .filter_map(|row| {
      let (head, rest) = row.split_first()?;
      let fields = match head {
        Pat::Ctor(other, args) if other == ctor => args.clone(),
        Pat::Wild => vec![Pat::Wild; arity],
        _ => return None,
      };

      Some(fields.into_iter().chain(rest.iter().cloned()).collect())
    })
    .collect()
}

/// The rows whose head matches anything, without it.
fn default_rows(rows: &[Vec<Pat>]) -> Vec<Vec<Pat>> {
  expand(rows).into_iter().filter(|row| row.first() == Some(&Pat::Wild)).map(|row| row.get(1..).unwrap_or_default().to_vec()).collect()
}

/// Puts the first `arity` patterns of `witness` back under `ctor`.
fn rebuild(ctor: &Ctor, arity: usize, witness: Vec<Pat>) -> Vec<Pat> {
  let split = arity.min(witness.len());
  let (args, rest) = witness.split_at(split);

  std::iter::once(Pat::Ctor(ctor.clone(), args.to_vec())).chain(rest.iter().cloned()).collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::IntTy;

  fn useful(rows: &[Pat], pat: Pat, ty: &Type) -> Option<Pat> {
    let typed = TypedModule::default();
    let space = Space { typed: &typed };
    let rows: Vec<Vec<Pat>> = rows.iter().map(|row| vec![row.clone()]).collect();

    space.useful(&rows, &[pat], std::slice::from_ref(ty)).and_then(|witness| witness.into_iter().next())
  }

  fn some(pat: Pat) -> Pat {
    Pat::Ctor(Ctor::Variant(1), vec![pat])
  }

  const NONE: Pat = Pat::Ctor(Ctor::Variant(0), Vec::new());

  #[test]
  fn finds_missing_variants_and_values() {
    let option_bool = Type::Option(Box::new(Type::Bool));
    let bool_pat = |value| Pat::Ctor(Ctor::Bool(value), Vec::new());

    assert_eq!(useful(&[NONE, some(bool_pat(true))], Pat::Wild, &option_bool), Some(some(bool_pat(false))));
    assert_eq!(useful(&[NONE, some(Pat::Wild)], Pat::Wild, &option_bool), None);
    assert_eq!(useful(&[Pat::Or(vec![NONE, some(Pat::Wild)])], Pat::Wild, &option_bool), None);

    let int = Type::Int(IntTy::I32);

    assert_eq!(useful(&[Pat::Ctor(Ctor::Int(1), Vec::new())], Pat::Wild, &int), Some(Pat::Wild));
  }

  #[test]
  fn finds_unreachable_rows() {
    let option_int = Type::Option(Box::new(Type::Int(IntTy::I32)));

    assert_eq!(useful(&[some(Pat::Wild)], some(Pat::Ctor(Ctor::Int(3), Vec::new())), &option_int), None);
    assert_eq!(useful(&[NONE, some(Pat::Wild)], Pat::Wild, &option_int), None);
    assert!(useful(&[NONE], some(Pat::Wild), &option_int).is_some());
  }
}
