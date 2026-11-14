// binary-trees: builds and walks many complete binary trees, as the Computer Language Benchmarks
// Game writes it. Allocation and garbage collection. The argument is the depth of the largest.
"use strict";

class Node {
  constructor(left, right) {
    this.left = left;
    this.right = right;
  }

  // The number of nodes of the tree.
  check() {
    return 1 + (this.left !== null ? this.left.check() : 0) + (this.right !== null ? this.right.check() : 0);
  }
}

function tree(depth) {
  return depth === 0 ? new Node(null, null) : new Node(tree(depth - 1), tree(depth - 1));
}

function main() {
  const requested = process.argv.length > 2 ? Number(process.argv[2]) : 10;
  const minDepth = 4;
  const maxDepth = requested < minDepth + 2 ? minDepth + 2 : requested;

  console.log(`stretch tree of depth ${maxDepth + 1}\t check: ${tree(maxDepth + 1).check()}`);

  const longLived = tree(maxDepth);

  for (let depth = minDepth; depth <= maxDepth; depth += 2) {
    const iterations = 2 ** (maxDepth - depth + minDepth);
    let check = 0;

    for (let i = 0; i < iterations; i++) {
      check += tree(depth).check();
    }

    console.log(`${iterations}\t trees of depth ${depth}\t check: ${check}`);
  }

  console.log(`long lived tree of depth ${maxDepth}\t check: ${longLived.check()}`);
}

main();
