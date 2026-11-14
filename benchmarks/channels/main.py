# channels: a producer thread sends `0..n` through a queue and the main thread receives and adds
# them. A queue, its lock and its waits. The argument is `n`.
import queue
import sys
import threading


def produce(channel, n):
    for i in range(n):
        channel.put(i)
    channel.put(None)


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 1000
    channel = queue.Queue()
    producer = threading.Thread(target=produce, args=(channel, n))
    producer.start()

    count = 0
    total = 0
    while True:
        value = channel.get()
        if value is None:
            break
        count += 1
        total += value

    producer.join()
    print(f"received {count} sum {total}")


main()
