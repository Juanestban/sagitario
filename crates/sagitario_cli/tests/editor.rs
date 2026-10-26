//! The VS Code extension in `editors/vscode/`
//! (docs/adr/0057-editor-tooling-and-independence-from-rust.md): its grammar highlights every
//! keyword of the language, so the two cannot drift apart.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// The keywords of `docs/spec/01-lexical.md`: the code block of the section "Keywords", and the
/// keywords reserved for later phases.
fn spec_keywords() -> io::Result<Vec<String>> {
  let spec = fs::read_to_string(root().join("docs/spec/01-lexical.md"))?;
  let section = spec.split_once("## Keywords").map(|(_, rest)| rest).unwrap_or_default();
  let block = section.split("```").nth(1).unwrap_or_default();
  let mut keywords: Vec<String> = block.split_whitespace().map(str::to_string).collect();

  keywords.extend(["async", "await"].map(str::to_string));

  Ok(keywords)
}

/// Whether `word` appears in `text` as a whole word, in a `match` of the grammar. The word
/// boundaries of the regular expressions (`\\b` in JSON) are not part of the words.
fn has_word(text: &str, word: &str) -> bool {
  let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';

  text.lines().filter(|line| line.contains("\"match\"")).map(|line| line.replace("\\\\b", " ")).any(|line| {
    line.match_indices(word).any(|(start, _)| {
      let before = line.get(..start).and_then(|text| text.chars().last());
      let after = line.get(start + word.len()..).and_then(|text| text.chars().next());

      !before.is_some_and(is_ident) && !after.is_some_and(is_ident)
    })
  })
}

#[test]
fn vscode_grammar_knows_every_keyword() -> io::Result<()> {
  let grammar = fs::read_to_string(root().join("editors/vscode/syntaxes/sagitario.tmLanguage.json"))?;
  let keywords = spec_keywords()?;
  let missing: Vec<&String> = keywords.iter().filter(|keyword| !has_word(&grammar, keyword)).collect();

  assert!(keywords.len() > 30, "the keywords of the spec were not found: {keywords:?}");
  assert!(missing.is_empty(), "keywords of docs/spec/01-lexical.md missing from the VS Code grammar: {missing:?}");

  Ok(())
}
