// channels: a producer thread sends `0..n` through a channel and the main thread receives and adds
// them. A channel, its lock and its waits. The argument is `n`.
use std::sync::mpsc;
use std::thread;

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|arg| arg.parse().ok()).unwrap_or(1000);
    let (sender, receiver) = mpsc::channel::<u64>();
    let producer = thread::spawn(move || {
        for i in 0..n {
            sender.send(i).unwrap();
        }
    });
    let (mut count, mut sum) = (0u64, 0u64);

    for value in receiver {
        count += 1;
        sum += value;
    }

    producer.join().unwrap();
    println!("received {} sum {}", count, sum);
}
