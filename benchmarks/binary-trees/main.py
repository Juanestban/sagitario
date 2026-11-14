# binary-trees: builds and walks many complete binary trees, as the Computer Language Benchmarks
# Game writes it. Allocation, reference counting and garbage collection. The argument is the
# depth of the largest.
import sys


class Node:
    __slots__ = ("left", "right")

    def __init__(self, left, right):
        self.left = left
        self.right = right

    def check(self):
        """The number of nodes of the tree."""
        left = self.left.check() if self.left is not None else 0
        right = self.right.check() if self.right is not None else 0
        return 1 + left + right


def tree(depth):
    if depth == 0:
        return Node(None, None)
    return Node(tree(depth - 1), tree(depth - 1))


def main():
    requested = int(sys.argv[1]) if len(sys.argv) > 1 else 10
    min_depth = 4
    max_depth = min_depth + 2 if requested < min_depth + 2 else requested

    print(f"stretch tree of depth {max_depth + 1}\t check: {tree(max_depth + 1).check()}")

    long_lived = tree(max_depth)

    for depth in range(min_depth, max_depth + 1, 2):
        iterations = 1 << (max_depth - depth + min_depth)
        check = 0
        for _ in range(iterations):
            check += tree(depth).check()
        print(f"{iterations}\t trees of depth {depth}\t check: {check}")

    print(f"long lived tree of depth {max_depth}\t check: {long_lived.check()}")


main()
