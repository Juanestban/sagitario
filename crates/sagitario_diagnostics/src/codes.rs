//! Registry of diagnostic codes. Every diagnostic the compiler emits uses one of these codes.
//!
//! Errors: `E0001`–`E0099` lexical, `E0100`–`E0199` syntax, `E0200`–`E0299` names,
//! `E0300`–`E0399` types and control flow, `E0900`–`E0999` language features not implemented
//! yet. Warnings: `W0001`–`W0999`.

use crate::Code;

/// A character that cannot start any token.
pub const UNEXPECTED_CHARACTER: Code = Code("E0001");
/// A string or template literal without its closing delimiter.
pub const UNTERMINATED_STRING: Code = Code("E0002");
/// A `/* */` comment without its closing `*/`.
pub const UNTERMINATED_BLOCK_COMMENT: Code = Code("E0003");
/// An unknown or malformed escape sequence.
pub const INVALID_ESCAPE: Code = Code("E0004");
/// A malformed numeric literal: bad digits, misplaced `_`, unknown suffix, or too large.
pub const INVALID_NUMBER: Code = Code("E0005");
/// A character literal that is empty, unterminated or holds more than one character.
pub const INVALID_CHAR_LITERAL: Code = Code("E0006");

/// A token that does not fit the grammar at that point.
pub const UNEXPECTED_TOKEN: Code = Code("E0100");
/// A keyword reserved for a future phase (`async`, `await`) used in the program.
pub const RESERVED_KEYWORD: Code = Code("E0101");
/// Chained non-associative operators, such as `a < b < c`.
pub const CHAINED_OPERATORS: Code = Code("E0102");

/// A name that does not refer to anything in scope.
pub const UNRESOLVED_NAME: Code = Code("E0200");
/// The same name declared twice in one scope.
pub const DUPLICATE_DEFINITION: Code = Code("E0201");
/// A declaration using a name reserved for a future phase (`Send`, `Sync`, `spawn`).
pub const RESERVED_NAME: Code = Code("E0202");
/// A program without a `main` function.
pub const MISSING_MAIN: Code = Code("E0203");
/// A `main` function with the wrong signature.
pub const INVALID_MAIN: Code = Code("E0204");

/// An expression whose type differs from the expected one.
pub const MISMATCHED_TYPES: Code = Code("E0300");
/// A call with the wrong number of arguments.
pub const WRONG_ARGUMENT_COUNT: Code = Code("E0301");
/// An integer literal that does not fit its type.
pub const INTEGER_OUT_OF_RANGE: Code = Code("E0302");
/// `break` or `continue` outside of a loop.
pub const BREAK_OUTSIDE_LOOP: Code = Code("E0303");
/// `break` with a value inside `while` or `for`.
pub const BREAK_WITH_VALUE: Code = Code("E0304");
/// An `as` conversion between types that cannot be converted.
pub const INVALID_CAST: Code = Code("E0305");
/// A value of a type that cannot be interpolated in a template string.
pub const NOT_INTERPOLABLE: Code = Code("E0306");
/// An assignment whose target is not a binding.
pub const INVALID_ASSIGNMENT_TARGET: Code = Code("E0307");
/// An assignment to a `const` that may already hold a value, or to a parameter.
pub const ASSIGN_TO_CONST: Code = Code("E0308");
/// A binding read before it is definitely assigned.
pub const UNASSIGNED_BINDING: Code = Code("E0309");
/// An operator applied to operands of a type it does not support.
pub const INVALID_OPERAND: Code = Code("E0310");
/// A binding declared without type and value that is never assigned.
pub const UNKNOWN_TYPE: Code = Code("E0311");

/// A language feature that is specified but not implemented yet.
pub const NOT_IMPLEMENTED: Code = Code("E0900");

/// A binding that is never read (names starting with `_` are exempt).
pub const UNUSED_BINDING: Code = Code("W0001");
/// A `var` that is never reassigned.
pub const UNNEEDED_VAR: Code = Code("W0002");
/// A statement that can never run.
pub const UNREACHABLE_CODE: Code = Code("W0003");

pub const ALL: &[Code] = &[
  UNEXPECTED_CHARACTER,
  UNTERMINATED_STRING,
  UNTERMINATED_BLOCK_COMMENT,
  INVALID_ESCAPE,
  INVALID_NUMBER,
  INVALID_CHAR_LITERAL,
  UNEXPECTED_TOKEN,
  RESERVED_KEYWORD,
  CHAINED_OPERATORS,
  UNRESOLVED_NAME,
  DUPLICATE_DEFINITION,
  RESERVED_NAME,
  MISSING_MAIN,
  INVALID_MAIN,
  MISMATCHED_TYPES,
  WRONG_ARGUMENT_COUNT,
  INTEGER_OUT_OF_RANGE,
  BREAK_OUTSIDE_LOOP,
  BREAK_WITH_VALUE,
  INVALID_CAST,
  NOT_INTERPOLABLE,
  INVALID_ASSIGNMENT_TARGET,
  ASSIGN_TO_CONST,
  UNASSIGNED_BINDING,
  INVALID_OPERAND,
  UNKNOWN_TYPE,
  NOT_IMPLEMENTED,
  UNUSED_BINDING,
  UNNEEDED_VAR,
  UNREACHABLE_CODE,
];

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn codes_are_well_formed_and_unique() {
    for (index, code) in ALL.iter().enumerate() {
      let digits = code.0.strip_prefix('E').or_else(|| code.0.strip_prefix('W')).unwrap();

      assert!(digits.len() == 4 && digits.bytes().all(|byte| byte.is_ascii_digit()), "malformed code {}", code.0);
      assert!(!ALL.iter().skip(index + 1).any(|other| other == code), "duplicate code {}", code.0);
    }
  }
}
