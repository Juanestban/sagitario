//! Writes `compiler/std/unicode.sag`: the properties and case mappings of Rust's `char`, as the
//! standard library of Sagitario has them (docs/adr/0093-everyday-methods.md, Q621). Run by hand
//! when Unicode changes, with the Rust toolchain whose `char` gives the data:
//!
//!     rustc -O tools/unicode.rs -o build/unicode && build/unicode > compiler/std/unicode.sag
//!
//! Every number is six hexadecimal digits, so a table is a string the library searches without
//! decoding it first. The program checks each table against `char` before printing it.

use std::fmt::Write;

const WIDTH: usize = 6;

/// The ranges of code points with a property, as `start, end` pairs.
fn ranges(property: impl Fn(char) -> bool) -> Vec<(u32, u32)> {
    let mut result: Vec<(u32, u32)> = Vec::new();

    for code in 0..=0x10FFFFu32 {
        let Some(c) = char::from_u32(code) else { continue };

        if !property(c) {
            continue;
        }

        match result.last_mut() {
            Some(last) if last.1 + 1 == code => last.1 = code,
            _ => result.push((code, code)),
        }
    }

    result
}

/// A case mapping: runs of characters mapped to one character each, `start, end, step, offset`
/// (the offset plus `0x800000`, so it is never negative), and the characters mapped to several,
/// `from, count, first, second, third`.
struct Mapping {
    runs: Vec<(u32, u32, u32, i64)>,
    expansions: Vec<(u32, Vec<u32>)>,
}

fn mapping(map: impl Fn(char) -> Vec<char>) -> Mapping {
    let mut singles: Vec<(u32, i64)> = Vec::new();
    let mut expansions: Vec<(u32, Vec<u32>)> = Vec::new();

    for code in 0..=0x10FFFFu32 {
        let Some(c) = char::from_u32(code) else { continue };
        let mapped = map(c);

        if mapped.len() == 1 && mapped[0] == c {
            continue;
        }

        if mapped.len() == 1 {
            singles.push((code, mapped[0] as i64 - code as i64));
        } else {
            expansions.push((code, mapped.iter().map(|&m| m as u32).collect()));
        }
    }

    // Consecutive singles with the same offset and the same step become one run.
    let mut runs: Vec<(u32, u32, u32, i64)> = Vec::new();
    let mut index = 0;

    while index < singles.len() {
        let (start, offset) = singles[index];
        let mut end = start;
        let mut step = 1;

        if index + 1 < singles.len() && singles[index + 1].1 == offset {
            let candidate = singles[index + 1].0 - start;

            if candidate <= 2 {
                step = candidate;

                while index + 1 < singles.len() && singles[index + 1].1 == offset && singles[index + 1].0 == end + step {
                    index += 1;
                    end = singles[index].0;
                }
            }
        }

        runs.push((start, end, step, offset));
        index += 1;
    }

    Mapping { runs, expansions }
}

fn number(out: &mut String, value: u32) {
    write!(out, "{value:06x}").unwrap();
}

fn table_of_ranges(list: &[(u32, u32)]) -> String {
    let mut out = String::new();

    for &(start, end) in list {
        number(&mut out, start);
        number(&mut out, end);
    }

    out
}

fn table_of_runs(list: &[(u32, u32, u32, i64)]) -> String {
    let mut out = String::new();

    for &(start, end, step, offset) in list {
        number(&mut out, start);
        number(&mut out, end);
        number(&mut out, step);
        number(&mut out, (offset + 0x800000) as u32);
    }

    out
}

fn table_of_expansions(list: &[(u32, Vec<u32>)]) -> String {
    let mut out = String::new();

    for (from, to) in list {
        assert!(to.len() <= 3);
        number(&mut out, *from);
        number(&mut out, to.len() as u32);

        for index in 0..3 {
            number(&mut out, to.get(index).copied().unwrap_or(0));
        }
    }

    out
}

/// What the library's search of a run table gives for `code`, to check the tables.
fn mapped_by(runs: &[(u32, u32, u32, i64)], expansions: &[(u32, Vec<u32>)], code: u32) -> Vec<u32> {
    if let Some((_, to)) = expansions.iter().find(|(from, _)| *from == code) {
        return to.clone();
    }

    for &(start, end, step, offset) in runs {
        if code >= start && code <= end && (code - start) % step == 0 {
            return vec![(code as i64 + offset) as u32];
        }
    }

    vec![code]
}

/// A constant holding `table`, split in lines of whole numbers.
fn constant(out: &mut String, name: &str, doc: &str, table: &str) {
    writeln!(out, "/// {doc}").unwrap();
    writeln!(out, "const {name}: String = \"\"").unwrap();

    let line = WIDTH * 16;
    let mut start = 0;

    while start < table.len() {
        let end = (start + line).min(table.len());

        writeln!(out, "  + \"{}\"", &table[start..end]).unwrap();
        start = end;
    }

    writeln!(out, "  ;").unwrap();
    writeln!(out).unwrap();
}

fn main() {
    let properties: [(&str, &str, fn(char) -> bool); 5] = [
        ("ALPHABETIC", "`Alphabetic`, as Rust's `char::is_alphabetic`.", char::is_alphabetic),
        ("NUMERIC", "The general categories `Nd`, `Nl` and `No`, as Rust's `char::is_numeric`.", char::is_numeric),
        ("UPPERCASE", "`Uppercase`, as Rust's `char::is_uppercase`.", char::is_uppercase),
        ("LOWERCASE", "`Lowercase`, as Rust's `char::is_lowercase`.", char::is_lowercase),
        ("WHITE_SPACE", "`White_Space`, as Rust's `char::is_whitespace`.", char::is_whitespace),
    ];
    let upper = mapping(|c| c.to_uppercase().collect());
    let lower = mapping(|c| c.to_lowercase().collect());

    for code in 0..=0x10FFFFu32 {
        if let Some(c) = char::from_u32(code) {
            let wanted_upper: Vec<u32> = c.to_uppercase().map(|m| m as u32).collect();
            let wanted_lower: Vec<u32> = c.to_lowercase().map(|m| m as u32).collect();

            assert_eq!(mapped_by(&upper.runs, &upper.expansions, code), wanted_upper, "{code:x}");
            assert_eq!(mapped_by(&lower.runs, &lower.expansions, code), wanted_lower, "{code:x}");
        }
    }

    let mut out = String::new();
    let version = char::UNICODE_VERSION;

    writeln!(out, "//! The Unicode properties and case mappings of characters, by code point, as Rust's `char` has").unwrap();
    writeln!(out, "//! them (Unicode {}.{}.{}). Generated by `tools/unicode.rs`; do not edit it by hand", version.0, version.1, version.2).unwrap();
    writeln!(out, "//! (docs/adr/0093-everyday-methods.md, Q621). Only the standard library names this module.").unwrap();
    writeln!(out, "//!").unwrap();
    writeln!(out, "//! Each table is a string of numbers of six hexadecimal digits: ranges are `start, end` pairs,").unwrap();
    writeln!(out, "//! runs of a case mapping are `start, end, step, offset + 0x800000`, and expansions are").unwrap();
    writeln!(out, "//! `from, count, first, second, third`, each sorted by its first number.").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "pub const VERSION: String = \"{}.{}.{}\";", version.0, version.1, version.2).unwrap();
    writeln!(out).unwrap();

    for (name, doc, property) in properties {
        constant(&mut out, name, doc, &table_of_ranges(&ranges(property)));
    }

    constant(&mut out, "TO_UPPER_RUNS", "The runs of `char::to_uppercase` that give one character.", &table_of_runs(&upper.runs));
    constant(&mut out, "TO_UPPER_EXPANSIONS", "The characters `char::to_uppercase` gives several for.", &table_of_expansions(&upper.expansions));
    constant(&mut out, "TO_LOWER_RUNS", "The runs of `char::to_lowercase` that give one character.", &table_of_runs(&lower.runs));
    constant(&mut out, "TO_LOWER_EXPANSIONS", "The characters `char::to_lowercase` gives several for.", &table_of_expansions(&lower.expansions));

    out.push_str(include_str!("unicode_search.sag"));
    print!("{out}");
}
