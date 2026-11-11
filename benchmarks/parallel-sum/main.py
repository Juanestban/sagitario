# parallel-sum: four threads add `i * i % 1000` over their quarter of `0..n`, and the main thread
# adds their results. Threads and joins; CPython's lock lets one thread run Python at a time. The
# argument is `n`.
import sys
import threading

THREADS = 4


def partial(start, end, sums, slot):
    total = 0
    for i in range(start, end):
        total += i * i % 1000
    sums[slot] = total


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 1000
    chunk = n // THREADS
    sums = [0] * THREADS
    threads = []

    for index in range(THREADS):
        start = index * chunk
        end = n if index == THREADS - 1 else start + chunk
        thread = threading.Thread(target=partial, args=(start, end, sums, index))
        thread.start()
        threads.append(thread)

    for thread in threads:
        thread.join()

    print(sum(sums))


main()
