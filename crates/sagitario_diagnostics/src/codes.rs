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
/// A struct member without `pub` or `prv`.
pub const MISSING_VISIBILITY: Code = Code("E0103");
/// An enum without variants, or a variant with empty `()` or `{}`.
pub const EMPTY_DECLARATION: Code = Code("E0104");
/// An unknown, repeated or misplaced attribute, or one with the wrong arguments.
pub const INVALID_ATTRIBUTE: Code = Code("E0105");

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
/// A `prv` member used outside its struct, enum or class.
pub const PRIVATE_MEMBER: Code = Code("E0205");
/// Alternatives of a `|` pattern that bind different names.
pub const INCONSISTENT_BINDINGS: Code = Code("E0206");
/// An item used outside the modules that can see it.
pub const PRIVATE_ITEM: Code = Code("E0207");
/// A C symbol that is not a valid C identifier, is reserved, or is declared incompatibly twice.
pub const INVALID_SYMBOL: Code = Code("E0208");

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
/// An assignment to a field or element of a value that is not mutable.
pub const IMMUTABLE_PLACE: Code = Code("E0312");
/// A method that modifies `self` called on a value that is not mutable.
pub const IMMUTABLE_RECEIVER: Code = Code("E0313");
/// A static function called as a method, or a method called as a static function.
pub const WRONG_CALL_FORM: Code = Code("E0314");
/// A struct literal that does not initialize every field.
pub const MISSING_FIELDS: Code = Code("E0315");
/// A field or method that the type does not have.
pub const UNKNOWN_MEMBER: Code = Code("E0316");
/// A struct that contains itself by value.
pub const RECURSIVE_TYPE: Code = Code("E0317");
/// A generic type used with the wrong number of type arguments.
pub const WRONG_TYPE_ARGUMENTS: Code = Code("E0318");
/// A `match` that does not cover every possible value.
pub const NON_EXHAUSTIVE_MATCH: Code = Code("E0319");
/// A pattern whose shape does not fit the value it matches.
pub const INVALID_PATTERN: Code = Code("E0320");
/// `?` on a value or in a function where it cannot propagate.
pub const INVALID_TRY: Code = Code("E0321");
/// A type that cannot cross the C boundary (or a pointer to a managed type).
pub const NOT_FFI_SAFE: Code = Code("E0322");
/// An operation that needs an `unsafe` block: calling C, dereferencing or offsetting a pointer.
pub const UNSAFE_REQUIRED: Code = Code("E0323");
/// A class whose initializer `new` is missing or declared wrongly.
pub const INVALID_INITIALIZER: Code = Code("E0324");
/// A class field not assigned by the initializer before `self` is used or `new` returns.
pub const UNINITIALIZED_FIELD: Code = Code("E0325");
/// Extending something that is not a class, a cycle of `extends`, an abstract class instantiated
/// or abstract methods left unimplemented.
pub const INVALID_INHERITANCE: Code = Code("E0326");
/// An `override` without a method to override or with another signature, a method replaced
/// without `override`, or a malformed abstract method.
pub const INVALID_OVERRIDE: Code = Code("E0327");
/// `weak` on something other than a `var` class field of type `Option<C>`, with `C` a class.
pub const INVALID_WEAK: Code = Code("E0328");
/// An `impl` or interface that breaks the rules of interfaces: not an interface, a missing or
/// mismatched member, a cycle of `extends`, or an interface mentioning `Self` used as a type.
pub const INVALID_IMPL: Code = Code("E0329");
/// A type argument that does not implement a bound of its parameter, a bound that is not an
/// interface, or generic instances that never end.
pub const UNSATISFIED_BOUND: Code = Code("E0330");

/// A language feature that is specified but not implemented yet.
pub const NOT_IMPLEMENTED: Code = Code("E0900");

/// A binding that is never read (names starting with `_` are exempt).
pub const UNUSED_BINDING: Code = Code("W0001");
/// A `var` that is never reassigned.
pub const UNNEEDED_VAR: Code = Code("W0002");
/// A statement that can never run.
pub const UNREACHABLE_CODE: Code = Code("W0003");
/// A `match` arm that can never match because earlier arms cover its values.
pub const UNREACHABLE_PATTERN: Code = Code("W0004");
/// A `use` whose name is never used.
pub const UNUSED_IMPORT: Code = Code("W0005");
/// A type in the signature or fields of an item that is visible in more places than the type.
pub const PRIVATE_INTERFACE: Code = Code("W0006");
/// An `unsafe` block without unsafe operations.
pub const UNUSED_UNSAFE: Code = Code("W0007");

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
  MISSING_VISIBILITY,
  EMPTY_DECLARATION,
  INVALID_ATTRIBUTE,
  UNRESOLVED_NAME,
  DUPLICATE_DEFINITION,
  RESERVED_NAME,
  MISSING_MAIN,
  INVALID_MAIN,
  PRIVATE_MEMBER,
  INCONSISTENT_BINDINGS,
  PRIVATE_ITEM,
  INVALID_SYMBOL,
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
  IMMUTABLE_PLACE,
  IMMUTABLE_RECEIVER,
  WRONG_CALL_FORM,
  MISSING_FIELDS,
  UNKNOWN_MEMBER,
  RECURSIVE_TYPE,
  WRONG_TYPE_ARGUMENTS,
  NON_EXHAUSTIVE_MATCH,
  INVALID_PATTERN,
  INVALID_TRY,
  NOT_FFI_SAFE,
  UNSAFE_REQUIRED,
  INVALID_INITIALIZER,
  UNINITIALIZED_FIELD,
  INVALID_INHERITANCE,
  INVALID_OVERRIDE,
  INVALID_WEAK,
  INVALID_IMPL,
  UNSATISFIED_BOUND,
  NOT_IMPLEMENTED,
  UNUSED_BINDING,
  UNNEEDED_VAR,
  UNREACHABLE_CODE,
  UNREACHABLE_PATTERN,
  UNUSED_IMPORT,
  PRIVATE_INTERFACE,
  UNUSED_UNSAFE,
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
