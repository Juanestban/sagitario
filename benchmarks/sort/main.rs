// sort: sorts the argument's number of integers from the generator MINSTD
// (`x = x * 48271 % 2147483647`), the same in every language, with the standard sort of each.

fn main() {
    let count: usize = std::env::args().nth(1).and_then(|arg| arg.parse().ok()).unwrap_or(1000);
    let mut values: Vec<u64> = Vec::new();
    let mut seed = 42u64;

    for _ in 0..count {
        seed = seed * 48271 % 2147483647;
        values.push(seed);
    }

    values.sort();

    let checksum: u64 = values.iter().step_by(1000).sum();

    println!("min {} max {} checksum {}", values[0], values[count - 1], checksum);
}
