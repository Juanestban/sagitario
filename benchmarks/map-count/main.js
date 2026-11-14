// map-count: counts how often each of 100000 keys comes, in a hash map of integers. The argument
// is the number of keys drawn by the generator MINSTD (`x = x * 48271 % 2147483647`), the same
// in every language.
"use strict";

function main() {
  const count = process.argv.length > 2 ? Number(process.argv[2]) : 1000;
  const counts = new Map();
  let seed = 42;

  for (let i = 0; i < count; i++) {
    seed = (seed * 48271) % 2147483647;
    const key = seed % 100000;
    counts.set(key, (counts.get(key) || 0) + 1);
  }

  // The largest count, at its smallest key.
  let most = 0;
  let mostKey = 0;
  for (const [key, value] of counts) {
    if (value > most || (value === most && key < mostKey)) {
      most = value;
      mostKey = key;
    }
  }

  let small = 0;
  for (let key = 0; key < 100; key++) {
    small += counts.get(key) || 0;
  }

  console.log(`keys ${counts.size} most ${most} at ${mostKey} small ${small}`);
}

main();
