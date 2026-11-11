# sort: sorts the argument's number of integers from the generator MINSTD
# (`x = x * 48271 % 2147483647`), the same in every language, with the standard sort of each.
import sys


def main():
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 1000
    values = []
    seed = 42

    for _ in range(count):
        seed = seed * 48271 % 2147483647
        values.append(seed)

    values.sort()

    checksum = sum(values[::1000])

    print(f"min {values[0]} max {values[count - 1]} checksum {checksum}")


main()
