// word-count: counts the words of a text and prints the most frequent. Strings and a map. The
// text has the argument's number of words, chosen from 2000 by the generator MINSTD
// (`x = x * 48271 % 2147483647`), the same in every language.
"use strict";

function main() {
  const count = process.argv.length > 2 ? Number(process.argv[2]) : 1000;
  const words = [];
  let seed = 42;

  for (let i = 0; i < count; i++) {
    seed = (seed * 48271) % 2147483647;
    words.push(`w${seed % 2000}`);
  }

  const text = words.join(" ");
  const counts = new Map();

  for (const word of text.split(" ")) {
    counts.set(word, (counts.get(word) || 0) + 1);
  }

  // The most frequent first, and words of the same count in the order of their bytes.
  const entries = [...counts.entries()];
  entries.sort((a, b) => (a[1] !== b[1] ? b[1] - a[1] : a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));

  console.log(`words ${count} distinct ${counts.size}`);

  for (const [word, n] of entries.slice(0, 5)) {
    console.log(`${word} ${n}`);
  }
}

main();
