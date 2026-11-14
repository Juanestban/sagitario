// sort: sorts the argument's number of integers from the generator MINSTD
// (`x = x * 48271 % 2147483647`), the same in every language, with the standard sort of each.
"use strict";

function main() {
  const count = process.argv.length > 2 ? Number(process.argv[2]) : 1000;
  const values = [];
  let seed = 42;

  for (let i = 0; i < count; i++) {
    seed = (seed * 48271) % 2147483647;
    values.push(seed);
  }

  values.sort((a, b) => a - b);

  let checksum = 0;
  for (let i = 0; i < count; i += 1000) {
    checksum += values[i];
  }

  console.log(`min ${values[0]} max ${values[count - 1]} checksum ${checksum}`);
}

main();
