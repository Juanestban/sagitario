// word-count: counts the words of a text and prints the most frequent. Strings and a map. The
// text has the argument's number of words, chosen from 2000 by the generator MINSTD
// (`x = x * 48271 % 2147483647`), the same in every language.
use std::collections::HashMap;
use std::fmt::Write;

fn main() {
    let count: u64 = std::env::args().nth(1).and_then(|arg| arg.parse().ok()).unwrap_or(1000);
    let mut text = String::new();
    let mut seed = 42u64;

    for i in 0..count {
        seed = seed * 48271 % 2147483647;

        if i > 0 {
            text.push(' ');
        }

        write!(text, "w{}", seed % 2000).unwrap();
    }

    let mut counts: HashMap<&str, u64> = HashMap::new();

    for word in text.split(' ') {
        *counts.entry(word).or_insert(0) += 1;
    }

    let mut entries: Vec<(&str, u64)> = counts.iter().map(|(word, n)| (*word, *n)).collect();

    // The most frequent first, and words of the same count in the order of their bytes.
    entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));

    println!("words {} distinct {}", count, counts.len());

    for (word, n) in entries.iter().take(5) {
        println!("{} {}", word, n);
    }
}
