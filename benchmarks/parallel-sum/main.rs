// parallel-sum: four threads add `i * i % 1000` over their quarter of `0..n`, and the main thread
// adds their results. Threads and joins. The argument is `n`.
use std::thread;

const THREADS: u64 = 4;

fn partial(start: u64, end: u64) -> u64 {
    let mut sum = 0u64;

    for i in start..end {
        sum += i * i % 1000;
    }

    sum
}

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|arg| arg.parse().ok()).unwrap_or(1000);
    let chunk = n / THREADS;
    let handles: Vec<_> = (0..THREADS)
        .map(|index| {
            let start = index * chunk;
            let end = if index == THREADS - 1 { n } else { start + chunk };

            thread::spawn(move || partial(start, end))
        })
        .collect();

    let total: u64 = handles.into_iter().map(|handle| handle.join().unwrap()).sum();

    println!("{}", total);
}
