//! The codes of the errors in `Nova.toml` and in a package's layout (docs/nova.md#errors). They
//! use the prefix `N`, apart from the language's `E` and `W` codes.

use sagitario_diagnostics::Code;

/// Text that is not the TOML subset `Nova.toml` uses.
pub const SYNTAX: Code = Code("N0001");
/// A key or table defined twice.
pub const DUPLICATE: Code = Code("N0002");
/// A key or table that `Nova.toml` does not have.
pub const UNKNOWN_KEY: Code = Code("N0003");
/// A required table or key that is missing.
pub const MISSING_KEY: Code = Code("N0004");
/// A value of the wrong type.
pub const WRONG_TYPE: Code = Code("N0005");
/// A package name that is not a lowercase identifier, or is reserved.
pub const INVALID_NAME: Code = Code("N0006");
/// A version that is not `MAJOR.MINOR.PATCH`.
pub const INVALID_VERSION: Code = Code("N0007");
/// A part of `Nova.toml` that a later version of Nova provides.
pub const NOT_AVAILABLE: Code = Code("N0008");
/// A package without exactly one of `src/main.sag` and `src/lib.sag`.
pub const INVALID_LAYOUT: Code = Code("N0009");

/// Every code, in order.
pub const ALL: [Code; 9] = [SYNTAX, DUPLICATE, UNKNOWN_KEY, MISSING_KEY, WRONG_TYPE, INVALID_NAME, INVALID_VERSION, NOT_AVAILABLE, INVALID_LAYOUT];
