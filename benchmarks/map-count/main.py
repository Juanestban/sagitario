# map-count: counts how often each of 100000 keys comes, in a hash map of integers. The argument
# is the number of keys drawn by the generator MINSTD (`x = x * 48271 % 2147483647`), the same in
# every language.
import sys


def main():
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 1000
    counts = {}
    seed = 42

    for _ in range(count):
        seed = seed * 48271 % 2147483647
        key = seed % 100000
        counts[key] = counts.get(key, 0) + 1

    # The largest count, at its smallest key.
    most = 0
    most_key = 0
    for key, value in counts.items():
        if value > most or (value == most and key < most_key):
            most = value
            most_key = key

    small = sum(counts.get(key, 0) for key in range(100))

    print(f"keys {len(counts)} most {most} at {most_key} small {small}")


main()
