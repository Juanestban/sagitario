// map-count: counts how often each of 100000 keys comes, in a hash map of integers. The argument
// is the number of keys drawn by the generator MINSTD (`x = x * 48271 % 2147483647`), the same
// in every language.
use std::collections::HashMap;

fn main() {
    let count: u64 = std::env::args().nth(1).and_then(|arg| arg.parse().ok()).unwrap_or(1000);
    let mut counts: HashMap<u64, u64> = HashMap::new();
    let mut seed = 42u64;

    for _ in 0..count {
        seed = seed * 48271 % 2147483647;
        *counts.entry(seed % 100000).or_insert(0) += 1;
    }

    // The largest count, at its smallest key.
    let (mut most, mut most_key) = (0u64, 0u64);

    for (&key, &value) in &counts {
        if value > most || (value == most && key < most_key) {
            most = value;
            most_key = key;
        }
    }

    let small: u64 = (0..100u64).map(|key| counts.get(&key).copied().unwrap_or(0)).sum();

    println!("keys {} most {} at {} small {}", counts.len(), most, most_key, small);
}
