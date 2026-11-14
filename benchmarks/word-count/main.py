# word-count: counts the words of a text and prints the most frequent. Strings and a dict. The
# text has the argument's number of words, chosen from 2000 by the generator MINSTD
# (`x = x * 48271 % 2147483647`), the same in every language.
import sys


def main():
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 1000
    words = []
    seed = 42

    for _ in range(count):
        seed = seed * 48271 % 2147483647
        words.append(f"w{seed % 2000}")

    text = " ".join(words)
    counts = {}

    for word in text.split(" "):
        counts[word] = counts.get(word, 0) + 1

    # The most frequent first, and words of the same count in the order of their bytes.
    entries = sorted(counts.items(), key=lambda entry: (-entry[1], entry[0]))

    print(f"words {count} distinct {len(counts)}")

    for word, n in entries[:5]:
        print(f"{word} {n}")


main()
